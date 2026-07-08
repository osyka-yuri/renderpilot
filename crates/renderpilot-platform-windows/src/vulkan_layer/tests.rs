use super::*;
use std::assert_matches;
use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

struct FakeRegistry {
    entries: RefCell<Vec<LayerRegistryEntry>>,
    can_write: bool,
}

impl Default for FakeRegistry {
    fn default() -> Self {
        Self {
            entries: RefCell::new(Vec::new()),
            can_write: true,
        }
    }
}

impl FakeRegistry {
    /// Registers a manifest entry with an explicit active/disabled flag
    /// under HKLM (the default hive).
    fn register_with(&self, manifest_path: &Path, active: bool) -> io::Result<()> {
        self.register_with_hive(manifest_path, active, RegistryHive::Hklm)
    }

    /// Registers a manifest entry under a specific hive (HKCU for tests
    /// that need to exercise the HKCU-only visibility caveat).
    fn register_with_hive(
        &self,
        manifest_path: &Path,
        active: bool,
        hive: RegistryHive,
    ) -> io::Result<()> {
        if !self.can_write {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        let mut entries = self.entries.borrow_mut();
        if !entries
            .iter()
            .any(|existing| same_path(&existing.manifest_path, manifest_path))
        {
            entries.push(LayerRegistryEntry {
                manifest_path: manifest_path.to_path_buf(),
                active,
                hive,
            });
        }
        Ok(())
    }
}

impl LayerRegistry for FakeRegistry {
    fn registered_layers(&self) -> Vec<LayerRegistryEntry> {
        self.entries.borrow().clone()
    }
    fn register(&self, manifest_path: &Path) -> io::Result<()> {
        self.register_with(manifest_path, true)
    }
    fn unregister(&self, manifest_path: &Path) -> io::Result<()> {
        self.entries
            .borrow_mut()
            .retain(|existing| !same_path(&existing.manifest_path, manifest_path));
        Ok(())
    }
    fn can_write_scope(&self) -> bool {
        self.can_write
    }
}

fn pe(machine: u16) -> Vec<u8> {
    let pe_offset = 0x80usize;
    let mut bytes = vec![0u8; pe_offset + 6];
    bytes[0..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&(pe_offset as u32).to_le_bytes());
    bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
    bytes[pe_offset + 4..pe_offset + 6].copy_from_slice(&machine.to_le_bytes());
    bytes
}

fn pe64() -> Vec<u8> {
    pe(0x8664)
}

/// Writes an external ReShade layer (manifest + DLL) into `dir` and returns
/// the manifest path, ready to register.
fn write_external_reshade(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let dll = dir.join("ReShade64.dll");
    std::fs::write(&dll, pe64()).unwrap();
    let manifest = dir.join("reshade_layer.json");
    std::fs::write(
            &manifest,
            format!(
                r#"{{"file_format_version":"1.0.0","layer":{{"name":"VK_LAYER_reshade","type":"GLOBAL","library_path":"{}"}}}}"#,
                dll.to_string_lossy().replace('\\', "\\\\")
            ),
        )
        .unwrap();
    manifest
}

#[test]
fn manifest_json_matches_reshade_format() {
    let json = layer_manifest_json();
    let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(value["file_format_version"], "1.0.0");
    assert_eq!(value["layer"]["name"], "VK_LAYER_reshade");
    assert_eq!(value["layer"]["type"], "GLOBAL");
    assert_eq!(value["layer"]["library_path"], r".\ReShade64.dll");
    assert_eq!(
        value["layer"]["disable_environment"]["DISABLE_VK_LAYER_reshade_1"],
        "1"
    );
    assert_eq!(
        value["layer"]["device_extensions"][0]["name"],
        "VK_EXT_tooling_info"
    );
}

#[test]
fn detect_is_absent_for_empty_registry() {
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    assert_eq!(detect(&registry, dir.path()), VulkanLayerState::Absent);
}

#[test]
fn detect_is_external_for_a_registered_reshade_layer() {
    let foreign = tempdir().unwrap();
    let manifest = write_external_reshade(foreign.path());
    let registry = FakeRegistry::default();
    registry.register(&manifest).unwrap();

    let our_dir = tempdir().unwrap();
    assert_eq!(
        detect(&registry, our_dir.path()),
        VulkanLayerState::External
    );
}

#[test]
fn detect_ignores_a_non_reshade_implicit_layer() {
    let other = tempdir().unwrap();
    let dll = other.path().join("mangohud.dll");
    std::fs::write(&dll, b"x").unwrap();
    let manifest = other.path().join("mangohud.json");
    std::fs::write(
        &manifest,
        format!(
            r#"{{"layer":{{"name":"VK_LAYER_MANGOHUD_overlay","library_path":"{}"}}}}"#,
            dll.to_string_lossy().replace('\\', "\\\\")
        ),
    )
    .unwrap();
    let registry = FakeRegistry::default();
    registry.register(&manifest).unwrap();

    let our_dir = tempdir().unwrap();
    assert_eq!(detect(&registry, our_dir.path()), VulkanLayerState::Absent);
}

#[test]
fn install_then_detect_is_installed() {
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    install(&registry, dir.path(), &pe64()).unwrap();

    assert!(dir.path().join(LAYER_DLL_NAME).is_file());
    assert!(dir.path().join(LAYER_JSON_NAME).is_file());
    assert_eq!(registry.registered_manifests().len(), 1);
    assert_eq!(detect(&registry, dir.path()), VulkanLayerState::Installed);
}

#[test]
fn duplicate_registry_views_for_same_standard_manifest_are_installed() {
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    install(&registry, dir.path(), &pe64()).unwrap();
    let manifest_path = dir.path().join(LAYER_JSON_NAME);
    registry
        .entries
        .borrow_mut()
        .push(LayerRegistryEntry::active(&manifest_path));

    let report = detect_report(&registry, dir.path());

    assert_eq!(report.state, VulkanLayerState::Installed);
    assert_eq!(
        report.facts.loader_visibility,
        VulkanLoaderVisibility::Normal
    );
    assert!(report.diagnostics.is_empty());
}

#[test]
fn registered_standard_manifest_without_files_is_conflict_missing_manifest() {
    // A registry key pointing to the standard location without any files
    // is a broken state (Conflict), not Absent — the registration exists
    // but the manifest file is missing. The UI offers reinstall.
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    registry
        .register(&dir.path().join(LAYER_JSON_NAME))
        .unwrap();
    let report = detect_report(&registry, dir.path());
    assert_eq!(report.state, VulkanLayerState::Conflict);
    assert_eq!(
        report.diagnostics,
        vec![VulkanLayerDiagnostic::MissingManifest]
    );
}

#[test]
fn files_exist_without_registry_key_is_conflict_registry_missing() {
    // Files on disk but no registry entry — the Vulkan loader can't find
    // the layer. This is a broken state with a RegistryMissing diagnostic.
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    install(&registry, dir.path(), &pe64()).unwrap();
    // Now simulate the registry key being deleted manually.
    registry.entries.borrow_mut().clear();

    let report = detect_report(&registry, dir.path());
    assert_eq!(report.state, VulkanLayerState::Conflict);
    assert!(report.facts.dll_path.is_some());
    assert!(report.facts.manifest_path.is_some());
    assert_eq!(
        report.diagnostics,
        vec![VulkanLayerDiagnostic::RegistryMissing]
    );
}

#[test]
fn no_files_no_registry_is_absent() {
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    let report = detect_report(&registry, dir.path());
    assert_eq!(report.state, VulkanLayerState::Absent);
}

#[test]
fn broken_layer_in_nonstandard_location_is_ignored() {
    // A leftover HKCU entry from a previous install (different directory)
    // with a missing DLL should NOT cause a Conflict — it's not our layer.
    let registry = FakeRegistry::default();
    let our_dir = tempdir().unwrap();

    // Register a broken manifest in a non-standard location.
    let old_dir = tempdir().unwrap();
    let old_manifest = old_dir.path().join("renderpilot-reshade-layer.json");
    std::fs::write(
            &old_manifest,
            r#"{"file_format_version":"1.0.0","layer":{"name":"VK_LAYER_reshade","type":"GLOBAL","library_path":"ReShade64.dll"}}"#,
        )
        .unwrap();
    // No DLL in the old directory → broken (MissingLayerDll).
    registry.register(&old_manifest).unwrap();

    let report = detect_report(&registry, our_dir.path());
    // The broken entry in the non-standard location is ignored → Absent.
    assert_eq!(report.state, VulkanLayerState::Absent);
}

#[test]
fn broken_layer_in_standard_location_without_dll_is_conflict() {
    // A manifest registered at the standard location, with the manifest
    // file present but the DLL missing, is a broken state (Conflict) —
    // the layer cannot load and needs reinstall.
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(dir.path()).unwrap();
    // Write the manifest file but not the DLL.
    std::fs::write(dir.path().join(LAYER_JSON_NAME), layer_manifest_json()).unwrap();
    registry
        .register(&dir.path().join(LAYER_JSON_NAME))
        .unwrap();

    let report = detect_report(&registry, dir.path());
    assert_eq!(report.state, VulkanLayerState::Conflict);
    assert_eq!(
        report.diagnostics,
        vec![VulkanLayerDiagnostic::MissingLayerDll]
    );
}

#[test]
fn permission_denied_dll_is_distinct_from_unreadable() {
    // When the DLL path is a directory (or otherwise triggers an access-
    // denied error on read), the detector must surface
    // `PermissionDenied` — not the generic `UnreadableDll` — so the UI
    // can tell the user this is a permission issue, not corruption.
    //
    // On Windows, `std::fs::read` on a directory returns
    // `ERROR_ACCESS_DENIED` (5) → `PermissionDenied`, which is exactly
    // what we need to exercise the split.
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(dir.path()).unwrap();
    // Write the manifest file.
    std::fs::write(dir.path().join(LAYER_JSON_NAME), layer_manifest_json()).unwrap();
    // Create a *directory* at the DLL path — reading it fails with
    // PermissionDenied on Windows.
    std::fs::create_dir_all(dir.path().join(LAYER_DLL_NAME)).unwrap();
    registry
        .register(&dir.path().join(LAYER_JSON_NAME))
        .unwrap();

    let report = detect_report(&registry, dir.path());
    assert_eq!(report.state, VulkanLayerState::Conflict);
    assert_eq!(
        report.diagnostics,
        vec![VulkanLayerDiagnostic::PermissionDenied]
    );
}

#[test]
fn disabled_registry_entry_is_installed_disabled() {
    // A standard-location layer with valid files but a disabled registry
    // entry (DWORD != 0) is InstalledDisabled, not Installed — the Vulkan
    // loader will skip it. Never reported as Current.
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    install(&registry, dir.path(), &pe64()).unwrap();
    // Replace the active entry with a disabled one.
    registry.entries.borrow_mut().clear();
    registry
        .register_with(&dir.path().join(LAYER_JSON_NAME), false)
        .unwrap();

    let report = detect_report(&registry, dir.path());
    assert_eq!(report.state, VulkanLayerState::InstalledDisabled);
    assert_eq!(
        report.diagnostics,
        vec![VulkanLayerDiagnostic::RegistryDisabled]
    );
}

#[test]
fn hkcu_only_registration_surfaces_visibility_caveat() {
    // A standard-location layer registered only under HKCU (not HKLM) is
    // a valid `Installed` state, but the loader may skip it for elevated
    // games. The detector must surface `HkcuNotVisibleWhenElevated` as
    // both a loader-visibility caveat and a diagnostic so the UI can warn.
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    install(&registry, dir.path(), &pe64()).unwrap();
    // Replace the HKLM entry with an HKCU-only one.
    registry.entries.borrow_mut().clear();
    registry
        .register_with_hive(&dir.path().join(LAYER_JSON_NAME), true, RegistryHive::Hkcu)
        .unwrap();

    let report = detect_report(&registry, dir.path());
    assert_eq!(report.state, VulkanLayerState::Installed);
    assert_eq!(
        report.facts.loader_visibility,
        VulkanLoaderVisibility::HkcuNotVisibleWhenElevated
    );
    assert!(
        report
            .diagnostics
            .contains(&VulkanLayerDiagnostic::HkcuNotVisibleWhenElevated)
    );
}

#[test]
fn hklm_registration_has_no_hkcu_caveat() {
    // A standard HKLM registration must NOT surface the HKCU caveat —
    // HKLM is visible to all processes including elevated ones.
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    install(&registry, dir.path(), &pe64()).unwrap();

    let report = detect_report(&registry, dir.path());
    assert_eq!(report.state, VulkanLayerState::Installed);
    assert_eq!(
        report.facts.loader_visibility,
        VulkanLoaderVisibility::Normal
    );
    assert!(
        !report
            .diagnostics
            .contains(&VulkanLayerDiagnostic::HkcuNotVisibleWhenElevated)
    );
}

#[test]
fn malformed_standard_manifest_is_conflict() {
    // A manifest file at the standard location that is corrupt JSON is a
    // broken state (Conflict) even if the DLL exists and the registry
    // entry is active — the Vulkan loader cannot parse the manifest.
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(dir.path()).unwrap();
    std::fs::write(dir.path().join(LAYER_DLL_NAME), pe64()).unwrap();
    std::fs::write(dir.path().join(LAYER_JSON_NAME), b"not valid json{").unwrap();
    registry
        .register(&dir.path().join(LAYER_JSON_NAME))
        .unwrap();

    let report = detect_report(&registry, dir.path());
    assert_eq!(report.state, VulkanLayerState::Conflict);
    assert_eq!(
        report.diagnostics,
        vec![VulkanLayerDiagnostic::ManifestMalformed]
    );
}

#[test]
fn foreign_manifest_malformed_json_is_reported_as_malformed() {
    // `detect_report` only surfaces a broken foreign (non-standard-location)
    // manifest when it's the standard one, so this exercises `inspect_manifest`
    // directly — the same unit `classify_manifest` delegates to for entries
    // outside the standard location.
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("reshade_layer.json");
    std::fs::write(&manifest_path, b"not valid json{").unwrap();

    match inspect_manifest(&manifest_path) {
        Err((diagnostic, _facts)) => {
            assert_eq!(diagnostic, VulkanLayerDiagnostic::ManifestMalformed);
        }
        Ok(_) => panic!("expected a malformed manifest to be reported as an error"),
    }
}

#[test]
fn foreign_manifest_permission_denied_is_distinct_from_malformed() {
    // A ReShade-looking foreign manifest that cannot be read due to a
    // permission error must be reported as `PermissionDenied`, not folded
    // into the generic `ManifestMalformed` diagnostic used for corrupt JSON —
    // the two point the user at different fixes. A directory at the manifest
    // path fails to read as a file (PermissionDenied on Windows), the same
    // technique `permission_denied_dll_is_distinct_from_unreadable` uses.
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("reshade_layer.json");
    std::fs::create_dir_all(&manifest_path).unwrap();

    match inspect_manifest(&manifest_path) {
        Err((diagnostic, _facts)) => {
            assert_eq!(diagnostic, VulkanLayerDiagnostic::PermissionDenied);
        }
        Ok(_) => panic!("expected an unreadable manifest to be reported as an error"),
    }
}

#[test]
fn uninstall_removes_files_and_registration() {
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    install(&registry, dir.path(), &pe64()).unwrap();

    uninstall(&registry, dir.path()).unwrap();
    assert!(!dir.path().exists());
    assert!(registry.registered_manifests().is_empty());
    assert_eq!(detect(&registry, dir.path()), VulkanLayerState::Absent);
}

#[test]
fn uninstall_is_ok_when_nothing_is_installed() {
    let registry = FakeRegistry::default();
    let dir = tempdir().unwrap();
    uninstall(&registry, &dir.path().join("missing")).unwrap();
}

#[test]
fn our_layer_wins_over_a_coexisting_external_entry() {
    let registry = FakeRegistry::default();
    let our_dir = tempdir().unwrap();
    install(&registry, our_dir.path(), &pe64()).unwrap();

    let foreign = tempdir().unwrap();
    let foreign_manifest = write_external_reshade(foreign.path());
    registry.register(&foreign_manifest).unwrap();

    assert_eq!(
        detect(&registry, our_dir.path()),
        VulkanLayerState::Conflict
    );
}

#[test]
fn register_app_creates_ini_and_tracks_exe() {
    let dir = tempdir().unwrap();
    let exe = Path::new(r"C:\Games\DOOM.exe");
    register_app(dir.path(), exe).unwrap();

    let apps = read_app_list(dir.path()).unwrap();
    assert_eq!(apps, vec![exe.to_path_buf()]);
}

#[test]
fn register_app_is_idempotent() {
    let dir = tempdir().unwrap();
    let exe = Path::new(r"C:\Games\DOOM.exe");
    register_app(dir.path(), exe).unwrap();
    register_app(dir.path(), exe).unwrap();

    let apps = read_app_list(dir.path()).unwrap();
    assert_eq!(apps.len(), 1);
}

#[test]
fn register_app_supports_multiple_games() {
    let dir = tempdir().unwrap();
    register_app(dir.path(), Path::new(r"C:\Games\game1.exe")).unwrap();
    register_app(dir.path(), Path::new(r"C:\Games\game2.exe")).unwrap();

    let apps = read_app_list(dir.path()).unwrap();
    assert_eq!(apps.len(), 2);
}

#[test]
fn unregister_app_returns_true_when_list_empty() {
    let dir = tempdir().unwrap();
    let exe = Path::new(r"C:\Games\DOOM.exe");
    register_app(dir.path(), exe).unwrap();

    let empty = unregister_app(dir.path(), exe).unwrap();
    assert!(empty);
    assert!(!dir.path().join(APPS_INI_NAME).exists());
}

#[test]
fn unregister_app_returns_false_when_others_remain() {
    let dir = tempdir().unwrap();
    let game1 = dir.path().join("game1.exe");
    let game2 = dir.path().join("game2.exe");
    std::fs::write(&game1, b"game1").unwrap();
    std::fs::write(&game2, b"game2").unwrap();
    register_app(dir.path(), &game1).unwrap();
    register_app(dir.path(), &game2).unwrap();

    let empty = unregister_app(dir.path(), &game1).unwrap();
    assert!(!empty);

    let apps = read_app_list(dir.path()).unwrap();
    assert_eq!(apps, vec![game2]);
}

#[test]
fn unregister_app_returns_true_when_ini_is_missing() {
    let dir = tempdir().unwrap();

    let empty = unregister_app(dir.path(), Path::new(r"C:\Games\missing.exe")).unwrap();

    assert!(empty);
}

#[test]
fn unregister_app_returns_true_when_ini_has_no_apps_key() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join(APPS_INI_NAME), "[ReShade]\nEnabled=1\n").unwrap();

    let empty = unregister_app(dir.path(), Path::new(r"C:\Games\missing.exe")).unwrap();

    assert!(empty);
    assert!(!dir.path().join(APPS_INI_NAME).exists());
}

#[test]
fn unregister_app_prunes_stale_missing_apps_on_available_roots() {
    let dir = tempdir().unwrap();
    let live = dir.path().join("live.exe");
    let stale = dir.path().join("stale.exe");
    std::fs::write(&live, b"live").unwrap();
    write_app_list(dir.path(), &[live.clone(), stale]).unwrap();

    let empty = unregister_app(dir.path(), Path::new(r"C:\Games\removed.exe")).unwrap();

    assert!(!empty);
    assert_eq!(read_app_list(dir.path()).unwrap(), vec![live]);
}

#[test]
fn read_app_list_returns_empty_when_no_ini() {
    let dir = tempdir().unwrap();
    assert!(read_app_list(dir.path()).unwrap().is_empty());
}

/// Invalid UTF-8 content makes `read_to_string` fail with something other
/// than `NotFound` while `is_file()` still reports `true` — a portable stand-in
/// for a real permission-denied file that doesn't require ACL manipulation.
fn write_unreadable_ini(dir: &Path) {
    std::fs::write(dir.join(APPS_INI_NAME), [0xFF, 0xFE, 0x00]).unwrap();
}

#[test]
fn read_app_list_propagates_read_errors_other_than_missing_file() {
    let dir = tempdir().unwrap();
    write_unreadable_ini(dir.path());

    let error = read_app_list(dir.path()).unwrap_err();

    assert_ne!(error.kind(), io::ErrorKind::NotFound);
}

#[test]
fn register_app_fails_instead_of_overwriting_an_unreadable_app_list() {
    let dir = tempdir().unwrap();
    write_unreadable_ini(dir.path());

    let error = register_app(dir.path(), Path::new(r"C:\Games\DOOM.exe")).unwrap_err();

    assert_ne!(error.kind(), io::ErrorKind::NotFound);
}

#[test]
fn unregister_app_fails_instead_of_deleting_a_shared_layer_it_could_not_read() {
    let dir = tempdir().unwrap();
    write_unreadable_ini(dir.path());

    let error = unregister_app(dir.path(), Path::new(r"C:\Games\DOOM.exe")).unwrap_err();

    assert_ne!(error.kind(), io::ErrorKind::NotFound);
}

#[test]
fn write_app_list_leaves_no_tmp_file_behind() {
    let dir = tempdir().unwrap();
    write_app_list(dir.path(), &[PathBuf::from(r"C:\Games\DOOM.exe")]).unwrap();

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();

    assert_eq!(entries, vec![std::ffi::OsString::from(APPS_INI_NAME)]);
}

#[test]
fn resolve_library_path_handles_relative_with_dot_prefix() {
    let manifest = Path::new(r"C:\ProgramData\ReShade\ReShade64.json");
    let resolved = resolve_library_path(manifest, r".\ReShade64.dll");
    assert_eq!(resolved, Path::new(r"C:\ProgramData\ReShade\ReShade64.dll"));
}

#[test]
fn resolve_library_path_handles_absolute() {
    let manifest = Path::new(r"C:\ProgramData\ReShade\ReShade64.json");
    let resolved = resolve_library_path(manifest, r"C:\D:\Other\ReShade64.dll");
    assert_eq!(resolved, Path::new(r"C:\D:\Other\ReShade64.dll"));
}

#[test]
fn detect_report_surfaces_registry_scope_not_writable_when_cannot_write() {
    let registry = FakeRegistry {
        can_write: false,
        ..Default::default()
    };
    let dir = tempdir().unwrap();
    let report = detect_report(&registry, dir.path());
    assert_eq!(report.state, VulkanLayerState::Absent);
    assert_eq!(
        report.diagnostics,
        vec![VulkanLayerDiagnostic::RegistryScopeNotWritable]
    );
}

#[test]
fn install_returns_registry_scope_not_writable_when_cannot_write() {
    let registry = FakeRegistry {
        can_write: false,
        ..Default::default()
    };
    let dir = tempdir().unwrap();
    let error = install(&registry, dir.path(), &pe64()).unwrap_err();
    assert_matches!(error, LayerInstallError::RegistryScopeNotWritable);
}
