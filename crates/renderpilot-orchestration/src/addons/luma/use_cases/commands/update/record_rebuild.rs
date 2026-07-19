//! Rebuilds the refreshed [`InstalledAddon`] record's tracked paths after a
//! successful set-diff update.

use std::path::{Path, PathBuf};

use renderpilot_domain::{InstalledAddon, PathRef, TrackedSource, TrackedSourceRole};

use super::diff::surviving_payload_paths;
use super::rollback::SetDiffRollback;
use crate::ServiceError;
use crate::addons::luma::fetch::types::LumaPayload;
use crate::addons::luma::source;
use crate::paths::same_path;

/// Membership via [`same_path`] so Windows path-form drift cannot duplicate entries.
pub(super) fn path_refs_contain(list: &[PathRef], path: &Path) -> bool {
    list.iter()
        .any(|tracked| same_path(Path::new(tracked.as_str()), path))
}

pub(super) fn tracked_addon_source(asset: &str, payload: &LumaPayload) -> TrackedSource {
    TrackedSource::new(
        TrackedSourceRole::AddonPayload,
        source::asset_url(asset),
        payload.etag.clone(),
        payload.zip_digest.clone(),
    )
    .with_last_modified(payload.last_modified.clone())
}

/// Rebuilds the full `created_files`/`backed_up_files` lists for the refreshed
/// record: surviving recorded payload paths, freshly added paths (as
/// engine-confirmed by `rollback.added`, the authoritative source for what was
/// actually written), the active host slot -- which may be a path this record
/// never tracked before (A.3) -- host-adjacent ownership such as adopted
/// `ReShade.ini` (never on the payload set-diff surface), and the old
/// `backed_up_files` minus any `.bak` a removal just consumed (A.4), plus
/// whatever an addition shadowed (A.1).
pub(super) fn rebuild_record_paths(
    record: &InstalledAddon,
    removed: &[PathBuf],
    rollback: &SetDiffRollback,
    host_path: Option<&Path>,
    dependency_paths: &[PathBuf],
) -> Result<(Vec<PathRef>, Vec<PathRef>), ServiceError> {
    let mut created_files = Vec::new();
    // Survivors must not depend on strip_prefix/key success -- only same_path
    // removal / host / dependency exclusion (see diff::surviving_payload_paths).
    for path in surviving_payload_paths(record, dependency_paths)
        .into_iter()
        .filter(|path| !removed.iter().any(|r| same_path(r, path)))
    {
        created_files.push(crate::addons::record::to_path_ref(&path)?);
    }
    for path in dependency_paths {
        if record
            .created_files()
            .iter()
            .any(|tracked| same_path(Path::new(tracked.as_str()), path))
            && !path_refs_contain(&created_files, path)
        {
            created_files.push(crate::addons::record::to_path_ref(path)?);
        }
    }
    for path in &rollback.added.created_files {
        created_files.push(crate::addons::record::to_path_ref(path)?);
    }
    if let Some(host_path) = host_path
        && !path_refs_contain(&created_files, host_path)
    {
        created_files.push(crate::addons::record::to_path_ref(host_path)?);
    }
    // Adopted ReShade.ini (and other host-adjacent non-payload files) stay owned
    // for uninstall but are excluded from the ZIP set-diff -- re-attach them.
    for path in crate::addons::luma::tracking::owned_host_adjacent_paths(record) {
        if !path_refs_contain(&created_files, &path) {
            created_files.push(crate::addons::record::to_path_ref(&path)?);
        }
    }

    let consumed_backups: Vec<&Path> = rollback
        .removed
        .iter()
        .filter(|undo| undo.restored_original.is_some())
        .map(|undo| undo.path.as_path())
        .collect();
    let mut backed_up_files: Vec<PathRef> = record
        .backed_up_files()
        .iter()
        .filter(|path_ref| {
            let tracked = Path::new(path_ref.as_str());
            !consumed_backups
                .iter()
                .any(|consumed| same_path(tracked, consumed))
        })
        .cloned()
        .collect();
    for path in &rollback.added.backed_up_files {
        if !path_refs_contain(&backed_up_files, path) {
            backed_up_files.push(crate::addons::record::to_path_ref(path)?);
        }
    }

    Ok((created_files, backed_up_files))
}

#[cfg(test)]
mod tests {
    use crate::addons::luma::fetch::types::LumaPayloadFile;
    use renderpilot_domain::{AddonKind, GameId};
    use tempfile::tempdir;

    use super::super::rollback::RemovedFileUndo;
    use super::super::test_fixtures::path_ref;
    use super::*;

    #[test]
    fn refreshed_payload_source_replaces_advisory_identity_with_zip_provenance() {
        let payload = LumaPayload {
            files: vec![LumaPayloadFile {
                relative_path: "Luma-Game.addon".to_owned(),
                bytes: b"addon".to_vec(),
            }],
            main_addon_rel: "Luma-Game.addon".to_owned(),
            zip_digest: "verified-zip".to_owned(),
            etag: Some("\"etag\"".to_owned()),
            last_modified: None,
            build_number: None,
        };

        let source = tracked_addon_source("Luma-Game.zip", &payload);

        assert_eq!(source.digest(), "verified-zip");
        assert_eq!(source.etag(), Some("\"etag\""));
        assert!(!source.is_advisory());
    }

    #[test]
    fn rebuild_record_paths_includes_a_newly_written_host_path_even_when_never_tracked_before() {
        // A host written for the first time during an update must be retained
        // in ownership. The Luma caller reaches this helper only after its
        // explicit owned-proxy gate, never to adopt a reused user host.
        let dir = tempdir().expect("tempdir");
        let addon = dir.path().join("Luma-Game.addon");
        std::fs::write(&addon, b"addon").expect("write");
        let record = InstalledAddon::from_parts(
            GameId::new("steam:403640").expect("id"),
            AddonKind::Luma,
            path_ref(&addon),
            None,
            vec![path_ref(&addon)],
            Vec::new(),
            Vec::new(),
        )
        .expect("record");

        let rollback = SetDiffRollback::default();
        let host_path = dir.path().join("dxgi.dll");

        let (created_files, _) =
            rebuild_record_paths(&record, &[], &rollback, Some(&host_path), &[]).expect("rebuild");

        assert!(
            created_files
                .iter()
                .any(|p| Path::new(p.as_str()) == host_path),
            "a host written for the first time this update must be tracked"
        );
    }

    #[test]
    fn rebuild_record_paths_retains_adopted_reshade_ini_after_full_payload_diff() {
        // Host-adjacent ReShade.ini is excluded from the payload set-diff surface
        // so it never appears in `removed`; rebuild must still re-inject it into
        // created_files so uninstall keeps ownership.
        let dir = tempdir().expect("tempdir");
        let addon = dir.path().join("Luma-Game.addon");
        let host = dir.path().join("dxgi.dll");
        let ini = dir.path().join("ReShade.ini");
        std::fs::write(&addon, b"addon").expect("write");
        std::fs::write(&host, b"host").expect("write");
        std::fs::write(&ini, b"[GENERAL]\r\n").expect("write");

        let record = InstalledAddon::from_parts(
            GameId::new("steam:403640").expect("id"),
            AddonKind::Luma,
            path_ref(&addon),
            None,
            vec![path_ref(&addon), path_ref(&host), path_ref(&ini)],
            Vec::new(),
            Vec::new(),
        )
        .expect("record");

        let rollback = SetDiffRollback::default();
        let (created_files, _) =
            rebuild_record_paths(&record, &[], &rollback, Some(&host), &[]).expect("rebuild");

        assert!(
            created_files.iter().any(|p| Path::new(p.as_str())
                .file_name()
                .is_some_and(|n| n == "ReShade.ini")),
            "adopted ReShade.ini must remain owned after set-diff rebuild"
        );
        assert!(
            created_files.iter().any(|p| Path::new(p.as_str()) == host),
            "host path must remain owned"
        );
    }

    #[test]
    fn rebuild_record_paths_excludes_a_consumed_backup_from_backed_up_files() {
        // A.4: once a removal consumes a `.bak` (restoring the foreign original
        // live), the refreshed record must stop listing it as backed-up --
        // otherwise a later uninstall logs a false "backup missing" warning.
        let dir = tempdir().expect("tempdir");
        let addon = dir.path().join("Luma-Game.addon");
        std::fs::write(&addon, b"addon").expect("write");
        let shadowed_path = dir.path().join("nvngx_dlss.dll");

        let record = InstalledAddon::from_parts(
            GameId::new("steam:403640").expect("id"),
            AddonKind::Luma,
            path_ref(&addon),
            None,
            vec![path_ref(&addon), path_ref(&shadowed_path)],
            vec![path_ref(&shadowed_path)],
            Vec::new(),
        )
        .expect("record");

        let removed = vec![shadowed_path.clone()];
        let mut rollback = SetDiffRollback::default();
        rollback.removed.push(RemovedFileUndo {
            path: shadowed_path.clone(),
            payload_bytes: b"luma-dlss".to_vec(),
            restored_original: Some((
                crate::fs::backup_path(&shadowed_path).expect("bak path"),
                b"game-own-dlss".to_vec(),
            )),
        });

        let (_, backed_up_files) =
            rebuild_record_paths(&record, &removed, &rollback, None, &[]).expect("rebuild");

        assert!(
            !backed_up_files
                .iter()
                .any(|p| Path::new(p.as_str()) == shadowed_path),
            "a consumed backup must not remain listed as backed-up"
        );
    }

    #[test]
    fn rebuild_record_paths_excludes_consumed_backup_when_path_forms_differ() {
        let dir = tempdir().expect("tempdir");
        let addon = dir.path().join("Luma-Game.addon");
        std::fs::write(&addon, b"addon").expect("write");
        let shadowed = dir.path().join("nvngx_dlss.dll");
        std::fs::write(&shadowed, b"live").expect("write");
        // Record stores a `.`-component form; undo uses the clean join form.
        let stored = dir.path().join(".").join("nvngx_dlss.dll");

        let record = InstalledAddon::from_parts(
            GameId::new("steam:403640").expect("id"),
            AddonKind::Luma,
            path_ref(&addon),
            None,
            vec![path_ref(&addon), path_ref(&stored)],
            vec![path_ref(&stored)],
            Vec::new(),
        )
        .expect("record");

        let removed = vec![shadowed.clone()];
        let mut rollback = SetDiffRollback::default();
        rollback.removed.push(RemovedFileUndo {
            path: shadowed.clone(),
            payload_bytes: b"luma-dlss".to_vec(),
            restored_original: Some((
                crate::fs::backup_path(&shadowed).expect("bak path"),
                b"game-own-dlss".to_vec(),
            )),
        });

        let (_, backed_up_files) =
            rebuild_record_paths(&record, &removed, &rollback, None, &[]).expect("rebuild");

        assert!(
            backed_up_files.is_empty(),
            "same_path must drop the consumed backup even when PathBuf forms differ: {backed_up_files:?}"
        );
    }

    #[test]
    fn rebuild_record_paths_retains_unkeyed_survivor_outside_payload_dir() {
        // strip_prefix failure must not drop ownership -- survivors are absolute paths.
        let dir = tempdir().expect("tempdir");
        let other = tempdir().expect("other");
        let addon = dir.path().join("Luma-Game.addon");
        let outside = other.path().join("stray.addon");
        std::fs::write(&addon, b"addon").expect("write");
        std::fs::write(&outside, b"stray").expect("write");

        let record = InstalledAddon::from_parts(
            GameId::new("steam:403640").expect("id"),
            AddonKind::Luma,
            path_ref(&addon),
            None,
            vec![path_ref(&addon), path_ref(&outside)],
            Vec::new(),
            Vec::new(),
        )
        .expect("record");
        let rollback = SetDiffRollback::default();

        let (created_files, _) =
            rebuild_record_paths(&record, &[], &rollback, None, &[]).expect("rebuild");

        assert!(
            created_files
                .iter()
                .any(|p| Path::new(p.as_str()) == outside),
            "paths that cannot be keyed under payload_dir must still be retained"
        );
    }

    #[test]
    fn rebuild_record_paths_preserves_existing_managed_dependency_paths() {
        let dir = tempdir().expect("tempdir");
        let addon = dir.path().join("Luma-Game.addon");
        let wrapper = dir.path().join("D3D9.dll");
        let config = dir.path().join("dgVoodoo.conf");
        let record = InstalledAddon::from_parts(
            GameId::new("steam:49520").expect("id"),
            AddonKind::Luma,
            path_ref(&addon),
            None,
            vec![path_ref(&addon), path_ref(&wrapper), path_ref(&config)],
            Vec::new(),
            Vec::new(),
        )
        .expect("record");
        let rollback = SetDiffRollback::default();

        let (created_files, _) = rebuild_record_paths(
            &record,
            &[],
            &rollback,
            None,
            &[wrapper.clone(), config.clone()],
        )
        .expect("rebuild");

        assert!(
            created_files
                .iter()
                .any(|path| Path::new(path.as_str()) == wrapper)
        );
        assert!(
            created_files
                .iter()
                .any(|path| Path::new(path.as_str()) == config)
        );
    }
}
