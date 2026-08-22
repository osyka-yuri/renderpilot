//! Pure observation and planning for the canonical shared ReShade Vulkan layer.
//!
//! The planner deliberately knows nothing about publication or rollback.  It
//! captures exact bytes for the three canonical files and the exact registry
//! value, then returns a deterministic participant set for the orchestration
//! layer to execute under its transaction lock.  Unknown files and unknown
//! lines in `ReShadeApps.ini` are never implicit deletion authority.

use std::cmp::Ordering;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::apps_ini::{
    AppListChange, AppListPlan, AppListPlanError, plan_register_app, plan_unregister_app,
};
use super::manifest::layer_manifest_json;
#[cfg(windows)]
use super::registry::WindowsLayerRegistry;
use super::registry::{LayerRegistry, RegistryValueState};
use super::{APPS_INI_NAME, LAYER_DLL_NAME, LAYER_JSON_NAME};

/// Exact bytes (or absence) of one canonical file participant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileObservation {
    /// The participant is absent.
    Absent,
    /// The participant exists with these exact bytes.
    Present(Vec<u8>),
}

impl FileObservation {
    fn is_present(&self) -> bool {
        matches!(self, Self::Present(_))
    }
}

/// The kind of an entry observed directly inside the shared layer directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DirectoryEntryKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A symbolic link.  It is preserved as unknown content.
    Symlink,
    /// Any other filesystem object.
    Other,
}

/// Exact directory observation used to prevent a planner from taking
/// ownership of unrelated files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntryObservation {
    /// Entry name relative to the observed layer directory.
    pub name: PathBuf,
    /// Entry kind at observation time.
    pub kind: DirectoryEntryKind,
}

/// Exact, complete observation of the shared layer directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryObservation {
    /// The directory existed at observation time.
    pub exists: bool,
    /// All entries visible at observation time, sorted by name.
    pub entries: Vec<DirectoryEntryObservation>,
}

impl DirectoryObservation {
    fn has_unknown_entries(&self) -> bool {
        self.entries.iter().any(|entry| {
            !matches!(
                entry.name.file_name().and_then(|name| name.to_str()),
                Some(LAYER_DLL_NAME | LAYER_JSON_NAME | APPS_INI_NAME)
            )
        })
    }

    fn has_only_canonical_entries(&self, keep_apps: bool) -> bool {
        self.entries.iter().all(|entry| {
            matches!(
                entry.name.file_name().and_then(|name| name.to_str()),
                Some(LAYER_DLL_NAME | LAYER_JSON_NAME)
            ) || (keep_apps
                && entry.name.file_name().and_then(|name| name.to_str()) == Some(APPS_INI_NAME))
        })
    }
}

/// Complete observation of all standard shared-layer participants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedVulkanLayerObservation {
    /// Directory whose canonical participants are being managed.
    pub layer_dir: PathBuf,
    /// Complete direct-child directory observation.
    pub directory: DirectoryObservation,
    /// Exact `ReShade64.dll` bytes.
    pub dll: FileObservation,
    /// Exact `ReShade64.json` bytes.
    pub manifest: FileObservation,
    /// Exact `ReShadeApps.ini` bytes.
    pub apps: FileObservation,
    /// Exact raw HKLM/64-bit registration value state.
    pub registry: RegistryValueState,
}

/// Reads all canonical shared-layer participants and the complete directory
/// entry set.  A read error is returned rather than interpreted as absence.
pub fn observe_shared_vulkan_layer(
    registry: &(impl LayerRegistry + ?Sized),
    layer_dir: &Path,
) -> io::Result<SharedVulkanLayerObservation> {
    let directory = observe_directory(layer_dir)?;
    let dll = observe_file(&layer_dir.join(LAYER_DLL_NAME))?;
    let manifest = observe_file(&layer_dir.join(LAYER_JSON_NAME))?;
    let apps = observe_file(&layer_dir.join(APPS_INI_NAME))?;
    let registry = registry.observe_canonical_registration(&layer_dir.join(LAYER_JSON_NAME))?;
    Ok(SharedVulkanLayerObservation {
        layer_dir: layer_dir.to_path_buf(),
        directory,
        dll,
        manifest,
        apps,
        registry,
    })
}

/// Observes the standard shared ReShade installation as one platform
/// authority.  The canonical ProgramData path and HKLM/64-bit registry view
/// are resolved here so callers cannot accidentally combine observations from
/// another layer directory or registry scope.
#[cfg(windows)]
pub fn observe_standard_shared_vulkan_layer() -> io::Result<SharedVulkanLayerObservation> {
    let layer_dir = super::reshade_common_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "the standard ReShade ProgramData directory is unavailable",
        )
    })?;
    observe_shared_vulkan_layer(&WindowsLayerRegistry, &layer_dir)
}

/// Returns the canonical manifest bytes used by every install and refresh
/// plan.  The value is intentionally exposed so the transaction manifest can
/// carry the exact postimage rather than regenerating it during publication.
pub fn canonical_manifest_bytes() -> Result<Vec<u8>, serde_json::Error> {
    layer_manifest_json().map(String::into_bytes)
}

/// Returns the exact active postimage for the canonical registry value:
/// `REG_DWORD` with native bytes for integer zero.
#[must_use]
pub fn active_registry_value() -> RegistryValueState {
    RegistryValueState::Present {
        // REG_DWORD is the stable Windows registry type identifier.
        value_type: 4,
        raw_bytes: vec![0; 4],
    }
}

/// The high-level operation represented by a pure layer plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerPlanOperation {
    /// Install/refresh canonical files and register one game executable.
    InstallAndRegister,
    /// Refresh canonical files and registration while preserving app tracking.
    Refresh,
    /// Add one executable to app tracking only.
    RegisterApp,
    /// Remove one executable, and remove the canonical layer only when it was
    /// proven to be the last tracked executable.
    UnregisterApp,
    /// Remove canonical DLL/manifest/registration while preserving app tracking.
    SettingsRemove,
}

/// Exact before/after file participant returned to orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMutation {
    /// Canonical live path.
    pub path: PathBuf,
    /// Exact observed bytes before publication.
    pub before: FileObservation,
    /// Exact desired bytes after publication, or absence.
    pub after: FileObservation,
}

/// Exact before/after registry participant returned to orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryMutation {
    /// Canonical manifest value name/path.
    pub manifest_path: PathBuf,
    /// Exact observed registry state.
    pub before: RegistryValueState,
    /// Exact desired registry state.
    pub after: RegistryValueState,
}

/// Directory-level authority returned by the planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryMutation {
    /// Shared layer directory.
    pub path: PathBuf,
    /// Complete observation used to derive this authority.
    pub before: DirectoryObservation,
    /// Whether the executor may create the directory before publishing files.
    pub create_if_absent: bool,
    /// Whether the executor may remove the directory after canonical cleanup.
    /// This is false whenever unknown content or preserved app tracking exists.
    pub remove_if_empty: bool,
}

/// Outcome of a strict app unregister observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppUnregisterOutcome {
    /// The target was not present in the unambiguous `Apps=` value.
    TargetAbsent,
    /// The target was removed and at least one other app remains.
    RemovedOthersRemain,
    /// The target was removed and no app remains; canonical layer removal is
    /// therefore authorized by this observation.
    RemovedLast,
    /// The input could not prove either state.  No mutation is authorized.
    Indeterminate,
}

/// A deterministic complete shared-layer plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedVulkanLayerPlan {
    /// Operation represented by this plan.
    pub operation: LayerPlanOperation,
    /// Exact file participants.  Unchanged participants are omitted.
    pub files: Vec<FileMutation>,
    /// Registry participant, omitted when already at the requested state.
    pub registry: Option<RegistryMutation>,
    /// Directory authority for creation and safe empty-directory cleanup.
    pub directory: DirectoryMutation,
    /// App unregister classification, when the operation unregisters an app.
    pub unregister_outcome: Option<AppUnregisterOutcome>,
}

impl SharedVulkanLayerPlan {
    /// Whether the plan contains no publication or directory action.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.files.is_empty()
            && self.registry.is_none()
            && !self.directory.create_if_absent
            && !self.directory.remove_if_empty
    }

    /// Whether the plan has proven that canonical layer removal is safe from
    /// the app-list perspective.
    #[must_use]
    pub fn authorizes_canonical_layer_removal(&self) -> bool {
        self.unregister_outcome == Some(AppUnregisterOutcome::RemovedLast)
    }
}

/// Errors specific to the pure shared-layer planner.
#[derive(Debug)]
pub enum LayerPlannerError {
    /// App tracking could not be parsed without risking data loss.
    AppList(AppListPlanError),
    /// Manifest generation failed.
    Manifest(serde_json::Error),
}

impl std::fmt::Display for LayerPlannerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AppList(error) => error.fmt(formatter),
            Self::Manifest(error) => {
                write!(formatter, "cannot serialize Vulkan layer manifest: {error}")
            }
        }
    }
}

impl std::error::Error for LayerPlannerError {}

/// Plans canonical installation plus registration of one executable.
pub fn plan_install_and_register(
    observation: SharedVulkanLayerObservation,
    dll_bytes: &[u8],
    exe_path: &Path,
) -> Result<SharedVulkanLayerPlan, LayerPlannerError> {
    let manifest = canonical_manifest_bytes().map_err(LayerPlannerError::Manifest)?;
    let app = plan_register_app_observed(&observation, exe_path)?;
    let dll_changed = !file_matches_bytes(&observation.dll, dll_bytes);
    let manifest_changed = !file_matches_bytes(&observation.manifest, &manifest);
    let registry_changed = observation.registry != active_registry_value();
    let SharedVulkanLayerObservation {
        layer_dir,
        directory,
        dll,
        manifest: current_manifest,
        apps,
        registry,
    } = observation;
    let mut plan = base_plan(layer_dir, directory, LayerPlanOperation::InstallAndRegister);
    if dll_changed {
        add_file(
            &mut plan.files,
            plan.directory.path.join(LAYER_DLL_NAME),
            dll,
            FileObservation::Present(dll_bytes.to_vec()),
        );
    }
    if manifest_changed {
        add_file(
            &mut plan.files,
            plan.directory.path.join(LAYER_JSON_NAME),
            current_manifest,
            FileObservation::Present(manifest),
        );
    }
    if registry_changed {
        add_registry(&mut plan, registry, active_registry_value());
    }
    let apps_path = plan.directory.path.join(APPS_INI_NAME);
    add_app_change(&mut plan, apps_path, apps, app);
    plan.directory.create_if_absent = !plan.directory.before.exists && !plan.is_noop();
    Ok(plan)
}

/// Plans a canonical file/registration refresh while preserving app tracking.
pub fn plan_refresh(
    observation: SharedVulkanLayerObservation,
    dll_bytes: &[u8],
) -> Result<SharedVulkanLayerPlan, LayerPlannerError> {
    let manifest = canonical_manifest_bytes().map_err(LayerPlannerError::Manifest)?;
    let dll_changed = !file_matches_bytes(&observation.dll, dll_bytes);
    let manifest_changed = !file_matches_bytes(&observation.manifest, &manifest);
    let registry_changed = observation.registry != active_registry_value();
    let SharedVulkanLayerObservation {
        layer_dir,
        directory,
        dll,
        manifest: current_manifest,
        apps: _,
        registry,
    } = observation;
    let mut plan = base_plan(layer_dir, directory, LayerPlanOperation::Refresh);
    if dll_changed {
        add_file(
            &mut plan.files,
            plan.directory.path.join(LAYER_DLL_NAME),
            dll,
            FileObservation::Present(dll_bytes.to_vec()),
        );
    }
    if manifest_changed {
        add_file(
            &mut plan.files,
            plan.directory.path.join(LAYER_JSON_NAME),
            current_manifest,
            FileObservation::Present(manifest),
        );
    }
    if registry_changed {
        add_registry(&mut plan, registry, active_registry_value());
    }
    plan.directory.create_if_absent = !plan.directory.before.exists && !plan.is_noop();
    Ok(plan)
}

/// Plans app registration without rewriting any other participant.
pub fn plan_register_app_only(
    observation: SharedVulkanLayerObservation,
    exe_path: &Path,
) -> Result<SharedVulkanLayerPlan, LayerPlannerError> {
    let app = plan_register_app_observed(&observation, exe_path)?;
    let SharedVulkanLayerObservation {
        layer_dir,
        directory,
        apps,
        dll: _,
        manifest: _,
        registry: _,
    } = observation;
    let mut plan = base_plan(layer_dir, directory, LayerPlanOperation::RegisterApp);
    let apps_path = plan.directory.path.join(APPS_INI_NAME);
    add_app_change(&mut plan, apps_path, apps, app);
    plan.directory.create_if_absent = !plan.directory.before.exists && !plan.is_noop();
    Ok(plan)
}

/// Plans unregistering one executable.  When the target is proven to be the
/// last tracked executable, canonical DLL/manifest/registry cleanup is part of
/// the same returned participant set.  `ReShadeApps.ini` remains present with
/// its now-empty value so comments and unknown keys are not discarded.
pub fn plan_unregister_app_only(
    observation: SharedVulkanLayerObservation,
    exe_path: &Path,
) -> Result<SharedVulkanLayerPlan, LayerPlannerError> {
    let app = plan_unregister_app_observed(&observation, exe_path)?;
    let SharedVulkanLayerObservation {
        layer_dir,
        directory,
        dll,
        manifest,
        apps,
        registry,
    } = observation;
    let mut plan = base_plan(layer_dir, directory, LayerPlanOperation::UnregisterApp);
    plan.unregister_outcome = Some(app.outcome);
    let apps_path = plan.directory.path.join(APPS_INI_NAME);
    add_app_change(&mut plan, apps_path, apps, app.app_plan);
    if app.outcome == AppUnregisterOutcome::RemovedLast {
        add_file(
            &mut plan.files,
            plan.directory.path.join(LAYER_DLL_NAME),
            dll,
            FileObservation::Absent,
        );
        add_file(
            &mut plan.files,
            plan.directory.path.join(LAYER_JSON_NAME),
            manifest,
            FileObservation::Absent,
        );
        add_registry(&mut plan, registry, RegistryValueState::Absent);
    }
    plan.directory.remove_if_empty = false;
    Ok(plan)
}

/// Plans settings-driven canonical layer removal while preserving the complete
/// `ReShadeApps.ini` byte sequence and every unrelated directory entry.
pub fn plan_settings_remove(observation: SharedVulkanLayerObservation) -> SharedVulkanLayerPlan {
    let SharedVulkanLayerObservation {
        layer_dir,
        directory,
        dll,
        manifest,
        apps,
        registry,
    } = observation;
    let keep_directory = directory.exists
        && !apps.is_present()
        && !directory.has_unknown_entries()
        && directory.has_only_canonical_entries(false);
    let mut plan = base_plan(layer_dir, directory, LayerPlanOperation::SettingsRemove);
    add_file(
        &mut plan.files,
        plan.directory.path.join(LAYER_DLL_NAME),
        dll,
        FileObservation::Absent,
    );
    add_file(
        &mut plan.files,
        plan.directory.path.join(LAYER_JSON_NAME),
        manifest,
        FileObservation::Absent,
    );
    add_registry(&mut plan, registry, RegistryValueState::Absent);
    plan.directory.remove_if_empty = keep_directory;
    plan
}

/// Classifies unregistering an app without authorizing a mutation.  Parse
/// failures are explicitly indeterminate and therefore fail closed.
#[must_use]
pub fn unregister_app_outcome(raw: Option<&[u8]>, exe_path: &Path) -> AppUnregisterOutcome {
    let Ok(app) = super::apps_ini::plan_unregister_app(raw, exe_path) else {
        return AppUnregisterOutcome::Indeterminate;
    };
    classify_app_plan(&app)
}

struct AppPlan {
    app_plan: AppListPlan,
    outcome: AppUnregisterOutcome,
}

fn plan_register_app_observed(
    observation: &SharedVulkanLayerObservation,
    exe_path: &Path,
) -> Result<AppListPlan, LayerPlannerError> {
    plan_register_app(file_bytes(&observation.apps), exe_path).map_err(LayerPlannerError::AppList)
}

fn plan_unregister_app_observed(
    observation: &SharedVulkanLayerObservation,
    exe_path: &Path,
) -> Result<AppPlan, LayerPlannerError> {
    let app_plan = plan_unregister_app(file_bytes(&observation.apps), exe_path)
        .map_err(LayerPlannerError::AppList)?;
    let outcome = classify_app_plan(&app_plan);
    Ok(AppPlan { app_plan, outcome })
}

fn classify_app_plan(plan: &AppListPlan) -> AppUnregisterOutcome {
    if matches!(plan.change, AppListChange::Unchanged) {
        AppUnregisterOutcome::TargetAbsent
    } else if plan.resulting_apps.is_empty() {
        AppUnregisterOutcome::RemovedLast
    } else {
        AppUnregisterOutcome::RemovedOthersRemain
    }
}

fn base_plan(
    layer_dir: PathBuf,
    directory: DirectoryObservation,
    operation: LayerPlanOperation,
) -> SharedVulkanLayerPlan {
    SharedVulkanLayerPlan {
        operation,
        files: Vec::new(),
        registry: None,
        directory: DirectoryMutation {
            path: layer_dir,
            before: directory,
            create_if_absent: false,
            remove_if_empty: false,
        },
        unregister_outcome: None,
    }
}

fn file_bytes(observation: &FileObservation) -> Option<&[u8]> {
    match observation {
        FileObservation::Absent => None,
        FileObservation::Present(bytes) => Some(bytes),
    }
}

fn add_app_change(
    plan: &mut SharedVulkanLayerPlan,
    path: PathBuf,
    before: FileObservation,
    app: AppListPlan,
) {
    let Some(bytes) = (match app.change {
        AppListChange::Unchanged => None,
        AppListChange::Replacement(bytes) => Some(bytes),
    }) else {
        return;
    };
    add_file(
        &mut plan.files,
        path,
        before,
        FileObservation::Present(bytes),
    );
}

fn add_file(
    files: &mut Vec<FileMutation>,
    path: PathBuf,
    before: FileObservation,
    after: FileObservation,
) {
    if before == after {
        return;
    }
    files.push(FileMutation {
        path,
        before,
        after,
    });
}

fn add_registry(
    plan: &mut SharedVulkanLayerPlan,
    before: RegistryValueState,
    after: RegistryValueState,
) {
    if before == after {
        return;
    }
    plan.registry = Some(RegistryMutation {
        manifest_path: plan.directory.path.join(LAYER_JSON_NAME),
        before,
        after,
    });
}

fn file_matches_bytes(observation: &FileObservation, bytes: &[u8]) -> bool {
    matches!(observation, FileObservation::Present(existing) if existing == bytes)
}

fn observe_file(path: &Path) -> io::Result<FileObservation> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(FileObservation::Absent);
        }
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "canonical Vulkan participant is not a regular file: {}",
                path.display()
            ),
        ));
    }
    fs::read(path).map(FileObservation::Present)
}

fn observe_directory(path: &Path) -> io::Result<DirectoryObservation> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(DirectoryObservation {
                exists: false,
                entries: Vec::new(),
            });
        }
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "shared Vulkan layer path is not a regular directory: {}",
                path.display()
            ),
        ));
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let metadata = fs::symlink_metadata(&entry_path)?;
        let kind = if metadata.file_type().is_symlink() {
            DirectoryEntryKind::Symlink
        } else if metadata.is_file() {
            DirectoryEntryKind::File
        } else if metadata.is_dir() {
            DirectoryEntryKind::Directory
        } else {
            DirectoryEntryKind::Other
        };
        entries.push(DirectoryEntryObservation {
            name: entry.file_name().into(),
            kind,
        });
    }
    entries.sort_by(|left, right| match left.name.cmp(&right.name) {
        Ordering::Equal => left.kind.cmp(&right.kind),
        order => order,
    });
    Ok(DirectoryObservation {
        exists: true,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;
    use crate::vulkan_layer::LayerRegistryEntry;

    struct FakeRegistry {
        value: RefCell<RegistryValueState>,
    }

    impl FakeRegistry {
        fn new(value: RegistryValueState) -> Self {
            Self {
                value: RefCell::new(value),
            }
        }
    }

    impl LayerRegistry for FakeRegistry {
        fn registered_layers(&self) -> Vec<LayerRegistryEntry> {
            Vec::new()
        }

        fn register(&self, _: &Path) -> io::Result<()> {
            Ok(())
        }

        fn unregister(&self, _: &Path) -> io::Result<()> {
            Ok(())
        }

        fn observe_canonical_registration(&self, _: &Path) -> io::Result<RegistryValueState> {
            Ok(self.value.borrow().clone())
        }

        fn activate_canonical_registration(&self, _: &Path) -> io::Result<()> {
            *self.value.borrow_mut() = active_registry_value();
            Ok(())
        }

        fn restore_canonical_registration(
            &self,
            _: &Path,
            state: &RegistryValueState,
        ) -> io::Result<()> {
            *self.value.borrow_mut() = state.clone();
            Ok(())
        }
    }

    struct ErrorRegistry;

    impl LayerRegistry for ErrorRegistry {
        fn registered_layers(&self) -> Vec<LayerRegistryEntry> {
            Vec::new()
        }

        fn register(&self, _: &Path) -> io::Result<()> {
            Ok(())
        }

        fn unregister(&self, _: &Path) -> io::Result<()> {
            Ok(())
        }

        fn observe_canonical_registration(&self, _: &Path) -> io::Result<RegistryValueState> {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected registry observation failure",
            ))
        }
    }

    fn observation(dir: &Path, registry: RegistryValueState) -> SharedVulkanLayerObservation {
        let fake = FakeRegistry::new(registry);
        observe_shared_vulkan_layer(&fake, dir).expect("observation")
    }

    #[test]
    fn registry_observation_errors_fail_closed() {
        let root = tempdir().expect("root");
        let error = observe_shared_vulkan_layer(&ErrorRegistry, root.path())
            .expect_err("registry observation failure must not become absence");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn canonical_file_observation_errors_fail_closed() {
        let root = tempdir().expect("root");
        let file = root.path().join("not-a-directory");
        std::fs::write(&file, b"not a directory").expect("file");
        let error =
            observe_shared_vulkan_layer(&FakeRegistry::new(RegistryValueState::Absent), &file)
                .expect_err("a non-directory canonical root must not become absence");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn canonical_postimages_are_stable_and_exact() {
        assert!(!canonical_manifest_bytes().unwrap().is_empty());
        assert_eq!(
            active_registry_value(),
            RegistryValueState::Present {
                value_type: 4,
                raw_bytes: vec![0, 0, 0, 0]
            }
        );
    }

    #[test]
    fn install_is_true_noop_when_everything_is_already_correct() {
        let root = tempdir().unwrap();
        let dll = b"dll";
        std::fs::write(root.path().join(LAYER_DLL_NAME), dll).unwrap();
        std::fs::write(
            root.path().join(LAYER_JSON_NAME),
            canonical_manifest_bytes().unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.path().join(APPS_INI_NAME),
            b"Apps=C:\\Games\\one.exe\n",
        )
        .unwrap();
        let observed = observation(root.path(), active_registry_value());
        let plan =
            plan_install_and_register(observed, dll, Path::new(r"C:\Games\one.exe")).unwrap();
        assert!(plan.is_noop());
        assert!(plan.files.is_empty());
        assert!(plan.registry.is_none());
    }

    #[test]
    fn install_becomes_app_list_only_when_layer_is_correct_but_app_is_missing() {
        let root = tempdir().unwrap();
        let dll = b"dll";
        std::fs::write(root.path().join(LAYER_DLL_NAME), dll).unwrap();
        std::fs::write(
            root.path().join(LAYER_JSON_NAME),
            canonical_manifest_bytes().unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.path().join(APPS_INI_NAME),
            b"; keep\nApps=C:\\Games\\one.exe\n",
        )
        .unwrap();
        let observed = observation(root.path(), active_registry_value());
        let plan =
            plan_install_and_register(observed, dll, Path::new(r"C:\Games\two.exe")).unwrap();
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].path.file_name().unwrap(), APPS_INI_NAME);
        assert_eq!(
            plan.files[0].after,
            FileObservation::Present(
                b"; keep\nApps=C:\\Games\\one.exe,C:\\Games\\two.exe\n".to_vec()
            )
        );
    }

    #[test]
    fn refresh_preserves_apps_and_unknown_directory_entries() {
        let root = tempdir().unwrap();
        std::fs::write(
            root.path().join(APPS_INI_NAME),
            b"; comment\nApps=C:\\Games\\one.exe\n",
        )
        .unwrap();
        std::fs::write(root.path().join("foreign.dat"), b"keep").unwrap();
        let observed = observation(root.path(), RegistryValueState::Absent);
        let plan = plan_refresh(observed, b"new dll").unwrap();
        assert_eq!(plan.files.len(), 2);
        assert!(plan.files.iter().all(|file| {
            matches!(
                file.path.file_name().and_then(|name| name.to_str()),
                Some(LAYER_DLL_NAME | LAYER_JSON_NAME)
            )
        }));
        assert!(!plan.directory.remove_if_empty);
    }

    #[test]
    fn refresh_moves_the_exact_dll_preimage_into_its_mutation() {
        let root = tempdir().unwrap();
        std::fs::write(root.path().join(LAYER_DLL_NAME), b"old dll").unwrap();
        let observed = observation(root.path(), RegistryValueState::Absent);
        let dll_ptr = match &observed.dll {
            FileObservation::Present(bytes) => bytes.as_ptr(),
            FileObservation::Absent => panic!("DLL observation must be present"),
        };

        let plan = plan_refresh(observed, b"new dll").unwrap();
        let before = plan
            .files
            .iter()
            .find(|file| {
                file.path.file_name().and_then(|name| name.to_str()) == Some(LAYER_DLL_NAME)
            })
            .map(|file| &file.before)
            .expect("refresh must publish the changed DLL");
        let before_ptr = match before {
            FileObservation::Present(bytes) => bytes.as_ptr(),
            FileObservation::Absent => panic!("DLL preimage must remain present"),
        };
        assert_eq!(before_ptr, dll_ptr);
    }

    #[test]
    fn unregister_distinguishes_absent_others_and_last() {
        let root = tempdir().unwrap();
        std::fs::write(
            root.path().join(APPS_INI_NAME),
            b"; keep\nApps=C:\\Games\\one.exe,C:\\Games\\two.exe\n",
        )
        .unwrap();
        std::fs::write(root.path().join(LAYER_DLL_NAME), b"dll").unwrap();
        std::fs::write(
            root.path().join(LAYER_JSON_NAME),
            canonical_manifest_bytes().unwrap(),
        )
        .unwrap();
        let observed = observation(root.path(), active_registry_value());
        let absent =
            plan_unregister_app_only(observed.clone(), Path::new(r"C:\Games\none.exe")).unwrap();
        assert_eq!(
            absent.unregister_outcome,
            Some(AppUnregisterOutcome::TargetAbsent)
        );
        assert!(absent.is_noop());

        let others =
            plan_unregister_app_only(observed.clone(), Path::new(r"C:\Games\one.exe")).unwrap();
        assert_eq!(
            others.unregister_outcome,
            Some(AppUnregisterOutcome::RemovedOthersRemain)
        );
        assert!(!others.authorizes_canonical_layer_removal());
        assert!(
            others.files.iter().all(|file| {
                !matches!(
                    file.path.file_name().and_then(|name| name.to_str()),
                    Some(LAYER_DLL_NAME | LAYER_JSON_NAME)
                )
            }),
            "another registered app must preserve the canonical layer files"
        );
        assert!(
            others.registry.is_none(),
            "another registered app must preserve the canonical registration"
        );

        let last_observed = SharedVulkanLayerObservation {
            apps: FileObservation::Present(b"; keep\nApps=C:\\Games\\one.exe\n".to_vec()),
            ..observed
        };
        let last = plan_unregister_app_only(last_observed, Path::new(r"C:\Games\one.exe")).unwrap();
        assert_eq!(
            last.unregister_outcome,
            Some(AppUnregisterOutcome::RemovedLast)
        );
        assert!(last.authorizes_canonical_layer_removal());
        assert!(last.files.iter().any(|file| {
            file.path.file_name().and_then(|name| name.to_str()) == Some(LAYER_DLL_NAME)
                && file.after == FileObservation::Absent
        }));
        assert!(last.files.iter().any(|file| {
            file.path.file_name().and_then(|name| name.to_str()) == Some(LAYER_JSON_NAME)
                && file.after == FileObservation::Absent
        }));
        assert!(last.files.iter().any(|file| {
            file.path.file_name().and_then(|name| name.to_str()) == Some(APPS_INI_NAME)
                && file.after == FileObservation::Present(b"; keep\nApps=\n".to_vec())
        }));
        assert_eq!(
            last.registry.as_ref().map(|registry| &registry.after),
            Some(&RegistryValueState::Absent),
            "the explicit last-app uninstall removes the canonical registration"
        );
    }

    #[test]
    fn invalid_apps_are_indeterminate_and_never_authorize_mutation() {
        let root = tempdir().unwrap();
        std::fs::write(root.path().join(APPS_INI_NAME), [0xff, 0xfe]).unwrap();
        let observed = observation(root.path(), active_registry_value());
        assert_eq!(
            unregister_app_outcome(file_bytes(&observed.apps), Path::new(r"C:\Games\one.exe")),
            AppUnregisterOutcome::Indeterminate
        );
        assert!(plan_unregister_app_only(observed, Path::new(r"C:\Games\one.exe")).is_err());
    }

    #[test]
    fn settings_remove_preserves_apps_and_unrelated_content() {
        let root = tempdir().unwrap();
        std::fs::write(
            root.path().join(APPS_INI_NAME),
            b"Apps=C:\\Games\\one.exe\n",
        )
        .unwrap();
        std::fs::write(root.path().join("foreign.ini"), b"keep").unwrap();
        std::fs::write(root.path().join(LAYER_DLL_NAME), b"dll").unwrap();
        std::fs::write(
            root.path().join(LAYER_JSON_NAME),
            canonical_manifest_bytes().unwrap(),
        )
        .unwrap();
        let observed = observation(root.path(), active_registry_value());
        let plan = plan_settings_remove(observed);
        assert!(plan.files.iter().any(|file| {
            file.path.file_name().and_then(|name| name.to_str()) == Some(LAYER_DLL_NAME)
                && file.after == FileObservation::Absent
        }));
        assert!(plan.files.iter().any(|file| {
            file.path.file_name().and_then(|name| name.to_str()) == Some(LAYER_JSON_NAME)
                && file.after == FileObservation::Absent
        }));
        assert!(!plan.files.iter().any(|file| {
            file.path.file_name().and_then(|name| name.to_str()) == Some(APPS_INI_NAME)
        }));
        assert!(!plan.directory.remove_if_empty);
    }

    #[test]
    fn directory_observation_keeps_unknown_kinds_and_sorts_entries() {
        let root = tempdir().unwrap();
        std::fs::write(root.path().join("z.dat"), b"z").unwrap();
        std::fs::write(root.path().join("a.dat"), b"a").unwrap();
        let observed = observation(root.path(), RegistryValueState::Absent);
        let names = observed
            .directory
            .entries
            .iter()
            .map(|entry| entry.name.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["a.dat", "z.dat"]);
    }
}
