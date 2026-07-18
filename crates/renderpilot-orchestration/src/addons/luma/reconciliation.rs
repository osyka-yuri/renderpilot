//! Recovery of a Luma payload left on disk after its database row was lost.
//!
//! Recovery is deliberately local-only. A `.addon` is not the release ZIP, so
//! recovery never fabricates ZIP provenance. The exact manifest payload name
//! lets it record an *advisory* content identity for the add-on plus `Luma/**`
//! tree; later update checks compare that same identity against a validated
//! release ZIP.

use std::path::{Path, PathBuf};

use renderpilot_application::InstalledAddonRepository;
#[cfg(test)]
use renderpilot_domain::ManagedFileMode;
use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, ManagedAddonFile,
    ManagedFileBaseline, PathRef, TrackedSource, TrackedSourceRole,
};

use crate::addons::records;
use crate::{Context, ServiceError};

use super::errors;
use super::{fetch, source};

/// Lazily upgrades the one legacy Luma-owned DLSS path into `managed_files`.
/// The caller must hold the game's mutation guard: this inspects live bytes,
/// classic sidecars and catalog claims before atomically replacing the record.
pub(crate) fn reconcile_legacy_dlss_binding_locked(
    context: &Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    record: &InstalledAddon,
) -> Result<InstalledAddon, ServiceError> {
    if guard.game_id() != record.game_id() {
        return Err(errors::failed(format!(
            "legacy Luma reconciliation guard belongs to {}, not {}",
            guard.game_id(),
            record.game_id()
        )));
    }
    if !record.managed_files().is_empty() {
        return Ok(record.clone());
    }
    let Some(path_ref) = super::dlss::find_created_dlss(record) else {
        return Ok(record.clone());
    };

    let live = Path::new(path_ref.as_str());
    renderpilot_detection::DlssBinaryInfo::from_path(live).map_err(|error| {
        repair_required(format!(
            "legacy Luma record claims {}, but the live DLL cannot be verified: {error}",
            live.display()
        ))
    })?;
    let installed_sha256 = renderpilot_detection::sha256_file(live)?;
    let sidecar =
        crate::fs::backup_path(live).map_err(|error| repair_required(error.to_string()))?;
    let legacy_backed = record
        .backed_up_files()
        .iter()
        .any(|path| crate::paths::same_path(Path::new(path.as_str()), live));

    let baseline = if legacy_backed {
        let baseline_sha256 = require_legacy_sidecar_hash(&sidecar)?;
        if baseline_sha256 == installed_sha256 {
            return Err(repair_required(format!(
                "legacy Luma backup {} is identical to the active DLSS bytes; the original baseline is no longer recoverable automatically",
                sidecar.display()
            )));
        }
        ManagedFileBaseline::Present {
            sha256: baseline_sha256,
        }
    } else if let Some(catalog_hash) = catalog_baseline_hash(context, record.game_id(), live)? {
        let sidecar_hash = require_legacy_sidecar_hash(&sidecar)?;
        if sidecar_hash != catalog_hash {
            return Err(repair_required(format!(
                "legacy DLSS sidecar {} contradicts the recorded catalog baseline",
                sidecar.display()
            )));
        }
        ManagedFileBaseline::Present {
            sha256: catalog_hash,
        }
    } else if sidecar.exists() {
        return Err(repair_required(format!(
            "legacy Luma record has an unclaimed classic sidecar at {}; its baseline cannot be inferred safely",
            sidecar.display()
        )));
    } else {
        ManagedFileBaseline::Absent
    };

    let binding = ManagedAddonFile::owned(path_ref.clone(), baseline, installed_sha256);
    let migrated = record
        .clone()
        .without_engine_managed_path(path_ref)
        .try_with_managed_files(vec![binding])
        .map_err(|error| repair_required(error.to_string()))?;
    context.storage().upsert_installed_addon(&migrated)?;
    Ok(migrated)
}

/// Catalog baseline hash for a legacy path, via the shared coordinated-file claim.
fn catalog_baseline_hash(
    context: &Context,
    game_id: &GameId,
    live: &Path,
) -> Result<Option<renderpilot_domain::Sha256Hash>, ServiceError> {
    let claim = crate::coordinated_files::catalog_path_claim(context.storage(), game_id, live)
        .map_err(|error| repair_required(error.to_string()))?;
    match claim.baseline() {
        Some(ManagedFileBaseline::Present { sha256 }) => Ok(Some(sha256.clone())),
        Some(ManagedFileBaseline::Absent) | None => Ok(None),
    }
}

/// Hard-fail vocabulary adapter over [`crate::fs::sha256_of_non_empty_file`].
fn require_legacy_sidecar_hash(
    sidecar: &Path,
) -> Result<renderpilot_domain::Sha256Hash, ServiceError> {
    crate::fs::sha256_of_non_empty_file(sidecar).map_err(|error| match error {
        crate::fs::NonEmptyFileError::Unreadable { .. } => repair_required(format!(
            "legacy Luma baseline {} is unavailable: {error}",
            sidecar.display()
        )),
        _ => repair_required(format!(
            "legacy Luma baseline is not a non-empty file: {}",
            sidecar.display()
        )),
    })
}

fn repair_required(message: impl Into<String>) -> ServiceError {
    errors::failed(format!("Luma DLSS repair required: {}", message.into()))
}

/// An on-disk Luma install found while no per-game DB row exists.
#[derive(Debug, Clone)]
pub(crate) struct OrphanedLumaInstall {
    pub(crate) game_id: GameId,
    /// Exact manifest asset backing `addon_file`.
    pub(crate) asset: String,
    pub(crate) addon_file: PathBuf,
    /// Files observed on disk and safe to remove with the adopted add-on.
    /// Always contains the exact add-on marker and `Luma/**` payload. May also
    /// include a proved-empty ReShade host and/or a CompatibleAdoptable
    /// dgVoodoo stack — never a foreign host or user-customized dgVoodoo conf.
    pub(crate) created_files: Vec<PathBuf>,
    /// Locally reconstructed provenance for a compatible empty ReShade host.
    /// It is present only when the caller proved that exact DLL is adopted and
    /// Luma's invariant nightly origin therefore applies.
    pub(crate) advisory_host_source: Option<TrackedSource>,
    /// Catalogue-pinned advisory wrapper provenance when the caller proved a
    /// Luma-shaped dgVoodoo stack (`CompatibleAdoptable`). Absent for
    /// user-reused configs (`CompatibleReusable`) so manage/update stay off.
    pub(crate) advisory_dgvoodoo_source: Option<TrackedSource>,
}

/// Adopts a local Luma payload into the per-game DB if the row is still absent.
/// This never downloads. It records a local content identity only after the
/// caller proved the exact manifest add-on file is present.
///
/// The caller must hold the per-game `game_mutation_lock` across reconciliation and
/// the availability snapshot. Keeping lock ownership at the orchestration
/// boundary avoids a nested `try_lock` silently turning adoption into a no-op.
pub(crate) fn reconcile_orphaned_install_locked(
    context: &Context,
    candidate: &OrphanedLumaInstall,
) -> Result<Option<InstalledAddon>, ServiceError> {
    if records::foreign_record(context, &candidate.game_id, AddonKind::Luma)?.is_some() {
        return Ok(None);
    }
    if let Some(record) = records::record_of_kind(context, &candidate.game_id, AddonKind::Luma)? {
        return Ok(Some(record));
    }

    let record = build_adopted_record(candidate)?;
    context.storage().upsert_installed_addon(&record)?;

    let record = records::record_of_kind(context, &candidate.game_id, AddonKind::Luma)?
        .ok_or_else(|| errors::failed("adopted Luma install was not persisted".to_owned()))?;
    Ok(Some(record))
}

fn build_adopted_record(candidate: &OrphanedLumaInstall) -> Result<InstalledAddon, ServiceError> {
    let payload_digest = fetch::digest::recovery_payload_digest_from_disk(
        &candidate.addon_file,
        &candidate.created_files,
    )?;
    let mut record = InstalledAddon::new(
        candidate.game_id.clone(),
        AddonKind::Luma,
        path_ref("add-on", &candidate.addon_file)?,
    )
    .with_host_kind(InstalledAddonHostKind::Proxy)
    .with_tracked_source(
        TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            source::asset_url(&candidate.asset),
            None,
            payload_digest,
        )
        .with_advisory(),
    );

    if let Some(host_source) = &candidate.advisory_host_source {
        record = record
            .with_tracked_source(host_source.clone())
            .with_reshade_channel("nightly");
    }
    if let Some(dgvoodoo_source) = &candidate.advisory_dgvoodoo_source {
        record = record.with_tracked_source(dgvoodoo_source.clone());
    }

    for path in &candidate.created_files {
        let path = path_ref("created file", path)?;
        if !record.created_files().contains(&path) {
            record = record.with_created_file(path);
        }
    }

    Ok(record)
}

fn path_ref(label: &str, path: &Path) -> Result<PathRef, ServiceError> {
    PathRef::new(path.to_string_lossy().into_owned())
        .map_err(|error| errors::failed(format!("invalid adopted {label} path: {error}")))
}

/// Discovers exactly the add-on marker expected for the resolved release asset,
/// plus the local `Luma/**` shader tree. Loose `Luma-*.addon` matching would
/// let an unrelated add-on be adopted after a manifest rename.
pub(crate) fn discover_orphaned_luma_payload(
    scan_dirs: &[&Path],
    expected_addon_name: &str,
) -> Option<(PathBuf, Vec<PathBuf>)> {
    for dir in scan_dirs {
        let Some(addon) = find_exact_file(dir, expected_addon_name) else {
            continue;
        };
        let mut created = vec![addon.clone()];
        let luma_dir = dir.join("Luma");
        if luma_dir.is_dir() {
            collect_files_recursive(&luma_dir, &mut created);
        }
        return Some((addon, created));
    }
    None
}

fn find_exact_file(dir: &Path, expected_name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let kind = entry.file_type().ok()?;
        let name = entry.file_name();
        (kind.is_file() && name.to_string_lossy().eq_ignore_ascii_case(expected_name))
            .then(|| entry.path())
    })
}

fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                out.push(path);
            } else if path.is_dir() {
                collect_files_recursive(&path, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn game_id() -> GameId {
        GameId::new("steam:403640").expect("id")
    }

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, bytes).expect("write");
    }

    #[test]
    fn discovery_requires_the_exact_resolved_addon_name() {
        let root = tempdir().expect("tmp");
        let dir = root.path();
        write(&dir.join("Luma-Other.addon"), b"other");
        write(&dir.join("Luma-Game.addon"), b"addon");
        write(&dir.join("Luma/Global/Copy_PS.hlsl"), b"hlsl");

        let (addon, created) =
            discover_orphaned_luma_payload(&[dir], "Luma-Game.addon").expect("discover");

        assert!(addon.ends_with("Luma-Game.addon"));
        assert!(created.iter().any(|path| path.ends_with("Copy_PS.hlsl")));
        assert!(
            discover_orphaned_luma_payload(&[dir], "Luma-Missing.addon").is_none(),
            "another Luma add-on must not be adopted under this profile"
        );
    }

    #[test]
    fn adopted_record_tracks_exact_payload_content_without_a_host_when_none_was_adopted() {
        let root = tempdir().expect("tmp");
        let addon = root.path().join("Luma-Game.addon");
        write(&addon, b"addon");
        write(&root.path().join("Luma/Global/Copy_PS.hlsl"), b"shader");

        let record = build_adopted_record(&OrphanedLumaInstall {
            game_id: game_id(),
            asset: "Luma-Game.zip".to_owned(),
            addon_file: addon.clone(),
            created_files: vec![addon, root.path().join("Luma/Global/Copy_PS.hlsl")],
            advisory_host_source: None,
            advisory_dgvoodoo_source: None,
        })
        .expect("record");

        let source = record
            .tracked_sources()
            .iter()
            .find(|source| source.role() == TrackedSourceRole::AddonPayload)
            .expect("advisory payload source");
        assert_eq!(source.url(), source::asset_url("Luma-Game.zip"));
        assert!(source.is_advisory());
        assert!(!source.digest().is_empty());
        assert!(record.has_addon_source());
        assert_eq!(record.reshade_channel(), None);
        assert!(record.registered_exe_path().is_none());
        assert!(
            record
                .tracked_sources()
                .iter()
                .all(|source| source.role() != TrackedSourceRole::DgVoodooWrapper)
        );
    }

    #[test]
    fn adopted_record_keeps_a_proved_empty_host_as_advisory_nightly() {
        let root = tempdir().expect("tmp");
        let addon = root.path().join("Luma-Game.addon");
        let host = root.path().join("dxgi.dll");
        write(&addon, b"addon");
        write(&host, b"reshade");
        let host_source = TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example.test/nightly.zip",
            None,
            "local-host-digest",
        )
        .with_channel("nightly")
        .with_advisory();

        let record = build_adopted_record(&OrphanedLumaInstall {
            game_id: game_id(),
            asset: "Luma-Game.zip".to_owned(),
            addon_file: addon.clone(),
            created_files: vec![addon, host],
            advisory_host_source: Some(host_source),
            advisory_dgvoodoo_source: None,
        })
        .expect("record");

        let host_source = record
            .tracked_sources()
            .iter()
            .find(|source| source.role() == TrackedSourceRole::HostBinary)
            .expect("host source");
        assert!(host_source.is_advisory());
        assert_eq!(host_source.channel(), Some("nightly"));
        assert_eq!(record.reshade_channel(), Some("nightly"));
        assert!(record.registered_exe_path().is_none());
    }

    #[test]
    fn adopted_record_keeps_compatible_adoptable_dgvoodoo_as_advisory_wrapper() {
        let root = tempdir().expect("tmp");
        let addon = root.path().join("Luma-Game.addon");
        let d3d9 = root.path().join("D3D9.dll");
        let config = root.path().join("dgVoodoo.conf");
        write(&addon, b"addon");
        write(&d3d9, b"dll");
        write(&config, b"[General]\r\n");
        let dgvoodoo_source = TrackedSource::new(
            TrackedSourceRole::DgVoodooWrapper,
            "https://example.test/dgVoodoo2.zip",
            None,
            "archive-digest",
        )
        .with_channel("dgvoodoo2@2.87.3")
        .with_advisory();

        let record = build_adopted_record(&OrphanedLumaInstall {
            game_id: game_id(),
            asset: "Luma-Game.zip".to_owned(),
            addon_file: addon.clone(),
            created_files: vec![addon, d3d9, config],
            advisory_host_source: None,
            advisory_dgvoodoo_source: Some(dgvoodoo_source),
        })
        .expect("record");

        let source = record
            .tracked_sources()
            .iter()
            .find(|source| source.role() == TrackedSourceRole::DgVoodooWrapper)
            .expect("dgvoodoo source");
        assert!(source.is_advisory());
        assert_eq!(source.channel(), Some("dgvoodoo2@2.87.3"));
        assert_eq!(source.digest(), "archive-digest");
        assert!(
            record
                .created_files()
                .iter()
                .any(|path| path.as_str().ends_with("D3D9.dll"))
        );
        assert!(
            record
                .created_files()
                .iter()
                .any(|path| path.as_str().ends_with("dgVoodoo.conf"))
        );
    }

    #[test]
    fn legacy_owned_dlss_moves_out_of_generic_engine_lists() {
        let root = tempdir().expect("tmp");
        let context = Context::open_at(root.path().join("catalog.sqlite")).expect("context");
        let addon = root.path().join("Luma-Game.addon");
        let live = root.path().join("nvngx_dlss.dll");
        let sidecar = root.path().join("nvngx_dlss.dll.bak");
        write(&addon, b"addon");
        write(
            &live,
            &crate::addons::luma::test_support::build_nvidia_dlss_pe([3, 7, 0, 0]),
        );
        write(
            &sidecar,
            &crate::addons::luma::test_support::build_nvidia_dlss_pe([2, 5, 0, 0]),
        );
        let record = InstalledAddon::new(
            game_id(),
            AddonKind::Luma,
            path_ref("addon", &addon).unwrap(),
        )
        .with_created_file(path_ref("live", &live).unwrap())
        .with_backed_up_file(path_ref("live", &live).unwrap());
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed");

        let guard = crate::game_mutation_lock::blocking_lock(record.game_id());
        let migrated =
            reconcile_legacy_dlss_binding_locked(&context, &guard, &record).expect("migrate");

        assert_eq!(migrated.managed_files().len(), 1);
        assert_eq!(migrated.managed_files()[0].mode(), ManagedFileMode::Owned);
        assert!(
            !migrated
                .created_files()
                .iter()
                .any(|path| path == &path_ref("live", &live).unwrap())
        );
        assert!(migrated.backed_up_files().is_empty());
    }

    #[test]
    fn legacy_identical_live_and_sidecar_requires_manual_repair() {
        let root = tempdir().expect("tmp");
        let context = Context::open_at(root.path().join("catalog.sqlite")).expect("context");
        let addon = root.path().join("Luma-Game.addon");
        let live = root.path().join("nvngx_dlss.dll");
        let sidecar = root.path().join("nvngx_dlss.dll.bak");
        let bytes = crate::addons::luma::test_support::build_nvidia_dlss_pe([3, 7, 0, 0]);
        write(&addon, b"addon");
        write(&live, &bytes);
        write(&sidecar, &bytes);
        let record = InstalledAddon::new(
            game_id(),
            AddonKind::Luma,
            path_ref("addon", &addon).unwrap(),
        )
        .with_created_file(path_ref("live", &live).unwrap())
        .with_backed_up_file(path_ref("live", &live).unwrap());

        let guard = crate::game_mutation_lock::blocking_lock(record.game_id());
        let error = reconcile_legacy_dlss_binding_locked(&context, &guard, &record)
            .expect_err("ambiguous baseline must fail closed");

        assert!(error.to_string().contains("repair required"));
        assert_eq!(std::fs::read(live).unwrap(), bytes);
        assert_eq!(std::fs::read(sidecar).unwrap(), bytes);
    }
}
