use std::path::{Path, PathBuf};

use renderpilot_domain::{
    InstalledAddon, PathRef, RenoDxInstallState, TrackedSource, TrackedSourceRole,
};

use crate::ServiceError;
use crate::addons::engine::InstallReceipt;

use super::errors;
use super::reshade;

pub(super) fn install_state_from_record(record: &InstalledAddon) -> RenoDxInstallState {
    RenoDxInstallState::Installed {
        version: record.addon_version().map(str::to_owned),
        reshade_managed_by_us: record.reshade_managed_by_us(),
        addon_dated: addon_dated_from_file_or_record(record),
        installed_at: record.installed_at(),
        updated_at: record.updated_at(),
        dlss_fix_installed: record.has_dlss_fix(),
        addon_tracked: record.has_addon_source(),
    }
}

fn addon_dated_from_file_or_record(record: &InstalledAddon) -> Option<String> {
    let path = Path::new(record.addon_file().as_str());
    if let Ok(modified) = std::fs::metadata(path).and_then(|metadata| metadata.modified()) {
        if crate::fs::is_reasonable_file_mtime(modified) {
            return Some(crate::fs::format_http_date(modified));
        }
    }
    record.addon_dated().map(str::to_owned)
}

pub(super) fn managed_host_path(record: &InstalledAddon) -> Option<PathBuf> {
    record
        .created_files()
        .iter()
        .chain(record.backed_up_files())
        .map(|path| PathBuf::from(path.as_str()))
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(reshade::is_proxy_slot)
        })
}

pub(super) fn required_managed_host_path(record: &InstalledAddon) -> Result<PathBuf, ServiceError> {
    managed_host_path(record).ok_or_else(|| {
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
        if source.role() == TrackedSourceRole::Host {
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
            "RenoDX install record has no managed ReShade host source".to_owned(),
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

pub(super) fn rebuild_after_receipt(
    record: &InstalledAddon,
    receipt: &InstallReceipt,
    removal: Option<(&Path, TrackedSourceRole)>,
    new_source: Option<TrackedSource>,
    label: &str,
) -> Result<InstalledAddon, ServiceError> {
    let mut created = record.created_files().to_vec();
    if let Some((removed_path, _)) = removal {
        let removed_str = removed_path.to_string_lossy();
        created.retain(|path| path.as_str() != removed_str);
    }
    merge_path_refs(&mut created, &receipt.created_files, label)?;

    let mut backed_up = record.backed_up_files().to_vec();
    merge_path_refs(&mut backed_up, &receipt.backed_up_files, label)?;

    let mut sources = record.tracked_sources().to_vec();
    if let Some((_, role)) = removal {
        sources.retain(|source| source.role() != role);
    }
    if let Some(source) = new_source {
        sources.push(source);
    }

    rebuild_with_parts(record, created, backed_up, sources, label)
}

fn rebuild_with_parts(
    record: &InstalledAddon,
    created_files: Vec<PathRef>,
    backed_up_files: Vec<PathRef>,
    tracked_sources: Vec<TrackedSource>,
    label: &str,
) -> Result<InstalledAddon, ServiceError> {
    let installed_at = record.installed_at();
    let updated_at = record.updated_at();
    InstalledAddon::from_parts(
        record.game_id().clone(),
        record.kind(),
        record.addon_file().clone(),
        record.addon_version().map(str::to_owned),
        created_files,
        backed_up_files,
        tracked_sources,
    )
    .map(|record| record.with_timestamps(installed_at, updated_at))
    .ok_or_else(|| errors::failed(format!("{label} violated the addon_file invariant")))
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
    use renderpilot_domain::{AddonKind, GameId, TrackedSourceRole};

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
    fn managed_host_path_reads_created_or_backed_up_proxy_slot() {
        let from_created = record(
            vec![
                path(r"C:\Games\Test\renodx-test.addon64"),
                path(r"C:\Games\Test\dxgi.dll"),
            ],
            Vec::new(),
        );
        assert_eq!(
            managed_host_path(&from_created).as_deref(),
            Some(Path::new("C:/Games/Test/dxgi.dll"))
        );

        let from_backup = record(
            vec![path(r"C:\Games\Test\renodx-test.addon64")],
            vec![path(r"C:\Games\Test\d3d11.dll")],
        );
        assert_eq!(
            required_managed_host_path(&from_backup)
                .expect("host")
                .file_name()
                .and_then(|name| name.to_str()),
            Some("d3d11.dll")
        );
    }

    #[test]
    fn replace_host_source_preserves_timestamps() {
        let source = TrackedSource::new(
            TrackedSourceRole::Host,
            "https://nightly.link/old.zip",
            None,
            "old",
        );
        let replacement = TrackedSource::new(
            TrackedSourceRole::Host,
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
        let first = TrackedSource::new(TrackedSourceRole::Host, "https://a", None, "a");
        let second = TrackedSource::new(TrackedSourceRole::Host, "https://b", None, "b");
        let record = record(vec![path(r"C:\Games\Test\renodx-test.addon64")], Vec::new())
            .with_tracked_source(first)
            .with_tracked_source(second);
        let replacement = TrackedSource::new(TrackedSourceRole::Host, "https://c", None, "c");

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
}
