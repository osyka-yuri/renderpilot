use std::path::{Path, PathBuf};

use renderpilot_domain::{
    InstalledAddon, PathRef, RenoDxHostKind, RenoDxInstallState, TrackedSource, TrackedSourceRole,
};

use crate::ServiceError;
use crate::addons::engine::InstallReceipt;
use crate::addons::tracking;

use super::errors;

/// Derives the `RenoDxInstallState` from the `managed_app_record` (the per-game `InstalledAddon`).
pub(super) fn install_state_from_record(record: &InstalledAddon) -> RenoDxInstallState {
    RenoDxInstallState::Installed {
        host_kind: record.host_kind().map(RenoDxHostKind::from),
        version: record.addon_version().map(str::to_owned),
        addon_dated: tracking::effective_addon_dated(record),
        installed_at: record.installed_at().unwrap_or(0),
        updated_at: record.updated_at().unwrap_or(0),
        dlss_fix_evidence_present: super::dlss_fix_binding::resolve(record).has_evidence,
        addon_tracked: record.has_addon_source(),
    }
}

pub(super) fn required_rollback_host_path(
    record: &InstalledAddon,
) -> Result<PathBuf, ServiceError> {
    tracking::host_proxy_path(record).ok_or_else(|| {
        errors::invalid("RenoDX install record does not identify a ReShade host path".to_owned())
    })
}

pub(super) fn replace_host_source(
    record: &InstalledAddon,
    replacement: &TrackedSource,
) -> Result<InstalledAddon, ServiceError> {
    let mut replaced = false;
    let mut sources = Vec::with_capacity(record.tracked_sources().len());
    for source in record.tracked_sources() {
        if source.role() == TrackedSourceRole::HostBinary {
            if replaced {
                return Err(errors::duplicate_host_sources());
            }
            sources.push(replacement.clone());
            replaced = true;
        } else {
            sources.push(source.clone());
        }
    }
    if !replaced {
        return Err(errors::invalid(
            "RenoDX install record has no ReShade host binary artifact".to_owned(),
        ));
    }
    rebuild_with_parts(
        record,
        record.created_files().to_vec(),
        record.backed_up_files().to_vec(),
        sources,
        "RenoDX host-source rebuild",
    )
}

pub(super) fn rebuild_with_sources_and_receipt(
    record: &InstalledAddon,
    sources: Vec<TrackedSource>,
    receipt: Option<&InstallReceipt>,
    label: &str,
) -> Result<InstalledAddon, ServiceError> {
    let mut created = record.created_files().to_vec();
    let mut backed_up = record.backed_up_files().to_vec();
    if let Some(receipt) = receipt {
        merge_path_refs(&mut created, &receipt.created_files, label)?;
        merge_path_refs(&mut backed_up, &receipt.backed_up_files, label)?;
    }
    rebuild_with_parts(record, created, backed_up, sources, label)
}

/// Rebuilds the single DLSS-Fix ownership projection. The exact managed path is
/// deletion authority; the source is advisory update provenance. Both are
/// replaced atomically in the persisted record so partial-evidence convergence
/// never leaves a source path pointing at an unrelated file.
pub(super) fn rebuild_with_dlss_projection(
    record: &InstalledAddon,
    exact_path: Option<&Path>,
    source: Option<TrackedSource>,
    label: &str,
) -> Result<InstalledAddon, ServiceError> {
    let expected = super::dlss_fix_binding::resolve(record).target;
    let expected_key = crate::paths::normalized_key(&expected);
    let mut created = record.created_files().to_vec();
    created.retain(|path| {
        let candidate = PathBuf::from(path.as_str());
        crate::paths::normalized_key(&candidate) != expected_key
    });
    if let Some(path) = exact_path {
        let path = PathRef::new(path.to_string_lossy().into_owned()).map_err(|error| {
            errors::failed(format!(
                "{label} produced an invalid DLSS-Fix path: {error}"
            ))
        })?;
        created.push(path);
    }
    let mut sources = record.tracked_sources().to_vec();
    sources.retain(|source| source.role() != TrackedSourceRole::DlssFix);
    if let Some(source) = source {
        sources.push(source);
    }
    rebuild_with_parts(
        record,
        created,
        record.backed_up_files().to_vec(),
        sources,
        label,
    )
}

fn rebuild_with_parts(
    record: &InstalledAddon,
    created_files: Vec<PathRef>,
    backed_up_files: Vec<PathRef>,
    tracked_sources: Vec<TrackedSource>,
    label: &str,
) -> Result<InstalledAddon, ServiceError> {
    tracking::rebuild_install_record(
        record,
        tracking::RebuildParts {
            addon_file: record.addon_file().clone(),
            addon_version: tracking::AddonVersionUpdate::Keep,
            managed_files: tracking::ManagedFilesUpdate::Keep,
            created_files,
            backed_up_files,
            tracked_sources,
            label: label.to_owned(),
        },
        tracking::PreserveMetadata::renodx(),
    )
}

fn merge_path_refs(
    target: &mut Vec<PathRef>,
    paths: &[PathBuf],
    label: &str,
) -> Result<(), ServiceError> {
    for path in paths {
        let path_ref = PathRef::new(path.to_string_lossy().into_owned()).map_err(|error| {
            errors::failed(format!(
                "{label} produced an invalid tracked path `{}`: {error}",
                path.display()
            ))
        })?;
        if !target.contains(&path_ref) {
            target.push(path_ref);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{AddonKind, GameId, InstalledAddonHostKind, TrackedSourceRole};

    use super::*;

    fn game_id() -> GameId {
        GameId::new("steam:42").expect("id")
    }

    fn path(value: &str) -> PathRef {
        PathRef::new(value).expect("path")
    }

    fn record(created: Vec<PathRef>, backed_up: Vec<PathRef>) -> InstalledAddon {
        InstalledAddon::from_parts(
            game_id(),
            AddonKind::RenoDx,
            path(r"C:\Games\Test\renodx-test.addon64"),
            None,
            created,
            backed_up,
            Vec::new(),
        )
        .expect("record")
    }

    #[test]
    fn rollback_host_path_reads_created_or_backed_up_proxy_slot() {
        let from_created = record(
            vec![
                path(r"C:\Games\Test\renodx-test.addon64"),
                path(r"C:\Games\Test\dxgi.dll"),
            ],
            Vec::new(),
        );
        assert_eq!(
            tracking::host_proxy_path(&from_created).as_deref(),
            Some(Path::new("C:/Games/Test/dxgi.dll"))
        );

        let from_backup = record(
            vec![path(r"C:\Games\Test\renodx-test.addon64")],
            vec![path(r"C:\Games\Test\d3d11.dll")],
        );
        assert_eq!(
            required_rollback_host_path(&from_backup)
                .expect("host")
                .file_name()
                .and_then(|name| name.to_str()),
            Some("d3d11.dll")
        );
    }

    #[test]
    fn replace_host_source_preserves_timestamps() {
        let source = TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://nightly.link/old.zip",
            None,
            "old",
        );
        let replacement = TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe",
            Some("\"etag\"".to_owned()),
            "new",
        )
        .with_channel("stable");
        let record = record(vec![path(r"C:\Games\Test\renodx-test.addon64")], Vec::new())
            .with_tracked_source(source)
            .with_timestamps(Some(10), Some(20));

        let updated = replace_host_source(&record, &replacement).expect("replace");

        assert_eq!(updated.tracked_sources(), &[replacement]);
        assert_eq!(updated.installed_at(), Some(10));
        assert_eq!(updated.updated_at(), Some(20));
    }

    #[test]
    fn replace_host_source_rejects_duplicate_host_sources() {
        let first = TrackedSource::new(TrackedSourceRole::HostBinary, "https://a", None, "a");
        let second = TrackedSource::new(TrackedSourceRole::HostBinary, "https://b", None, "b");
        let record = record(vec![path(r"C:\Games\Test\renodx-test.addon64")], Vec::new())
            .with_tracked_source(first)
            .with_tracked_source(second);
        let replacement = TrackedSource::new(TrackedSourceRole::HostBinary, "https://c", None, "c");

        assert!(replace_host_source(&record, &replacement).is_err());
    }

    #[test]
    fn rebuild_with_receipt_dedups_paths_and_preserves_timestamps() {
        let record = record(vec![path(r"C:\Games\Test\renodx-test.addon64")], Vec::new())
            .with_timestamps(Some(10), Some(20));
        let receipt = InstallReceipt {
            created_files: vec![
                PathBuf::from(r"C:\Games\Test\renodx-test.addon64"),
                PathBuf::from(r"C:\Games\Test\dxgi.dll"),
            ],
            backed_up_files: vec![PathBuf::from(r"C:\Games\Test\dxgi.dll")],
        };

        let updated =
            rebuild_with_sources_and_receipt(&record, Vec::new(), Some(&receipt), "test rebuild")
                .expect("rebuild");

        assert_eq!(updated.created_files().len(), 2);
        assert_eq!(updated.backed_up_files().len(), 1);
        assert_eq!(updated.installed_at(), Some(10));
        assert_eq!(updated.updated_at(), Some(20));
    }

    #[test]
    fn rebuild_with_receipt_preserves_host_metadata() {
        let record = record(vec![path(r"C:\Games\Test\renodx-test.addon64")], Vec::new())
            .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer)
            .with_reshade_channel("stable")
            .with_registered_exe_path(path(r"C:\Games\Test\Game.exe"));
        let receipt = InstallReceipt::default();

        let updated =
            rebuild_with_sources_and_receipt(&record, Vec::new(), Some(&receipt), "test rebuild")
                .expect("rebuild");

        assert_eq!(
            updated.host_kind(),
            Some(InstalledAddonHostKind::SharedVulkanLayer)
        );
        assert_eq!(updated.reshade_channel(), Some("stable"));
        assert_eq!(
            updated.registered_exe_path().map(PathRef::as_str),
            Some("C:/Games/Test/Game.exe")
        );
    }
}
