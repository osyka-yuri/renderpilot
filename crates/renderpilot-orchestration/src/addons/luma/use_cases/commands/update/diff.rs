//! Diffs a freshly fetched release payload against a Luma install's recorded
//! payload paths.
//!
//! Set membership (`added` / `removed`) is pure algebra over the record and
//! in-memory ZIP entries. Paths present in both sets are compared against the
//! live file in [`compute_diff`] so identical on-disk bytes are not cloned into
//! `changed` (and therefore not rewritten).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use renderpilot_domain::InstalledAddon;

use crate::addons::file_update::Replacement;
use crate::addons::luma::fetch::types::LumaPayloadFile;
use crate::addons::luma::tracking;

/// A relative path, `/`-normalized and lowercased, used as the set-diff key --
/// stable across the exact byte-casing quirks a rolling upstream ZIP might vary
/// between releases.
fn diff_key(relative_path: &str) -> String {
    relative_path.replace('\\', "/").to_ascii_lowercase()
}

/// Best-effort `path` relative to `root`, tolerant of path-form drift
/// (casing / `.` components / long-path prefixes) by falling back to
/// canonicalized strip when the syntactic prefix fails.
pub(super) fn relative_under(root: &Path, path: &Path) -> Option<PathBuf> {
    if let Ok(relative) = path.strip_prefix(root) {
        return Some(relative.to_path_buf());
    }
    let root_canon = crate::paths::canonicalize_best_effort(root);
    let path_canon = crate::paths::canonicalize_best_effort(path);
    path_canon
        .strip_prefix(&root_canon)
        .ok()
        .map(Path::to_path_buf)
}

/// Tracked payload paths excluding the host slot and managed dependencies.
/// Does **not** require a successful relative-key derivation -- used by record
/// rebuild so path-form drift cannot drop ownership.
pub(super) fn surviving_payload_paths(
    record: &InstalledAddon,
    excluded_paths: &[PathBuf],
) -> Vec<PathBuf> {
    tracking::payload_owned_paths(record, excluded_paths)
}

/// The install's tracked payload files (its `created_files`, excluding the
/// active host slot -- tracked and updated separately) as `(diff_key,
/// absolute path)` pairs. Host and managed dependency paths are excluded via
/// [`same_path`] so path-form drift (casing / long-path prefixes) cannot pull
/// them into the payload set-diff and get them deleted as "removed".
///
/// Paths that cannot be keyed under `payload_dir` are **excluded from the diff
/// only** (with a warning). Record rebuild must use
/// [`surviving_payload_paths`] so those paths are not dropped from ownership.
pub(super) fn recorded_payload_keys(
    record: &InstalledAddon,
    payload_dir: &Path,
    excluded_paths: &[PathBuf],
) -> Vec<(String, PathBuf)> {
    surviving_payload_paths(record, excluded_paths)
        .into_iter()
        .filter(|path| {
            !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.eq_ignore_ascii_case(crate::addons::luma::dlss::NVNGX_DLSS_FILE_NAME)
                })
        })
        .filter_map(|path| {
            let relative = match relative_under(payload_dir, &path) {
                Some(relative) => relative,
                None => {
                    log::warn!(
                        "Luma update set-diff: tracked path `{}` is not under payload dir `{}`; \
                         excluding from payload diff rather than risking a wrong removal",
                        path.display(),
                        payload_dir.display()
                    );
                    return None;
                }
            };
            Some((diff_key(&relative.to_string_lossy()), path))
        })
        .collect()
}

/// The result of diffing a freshly fetched payload against the record's
/// tracked payload paths.
///
/// - `added` -- only in the fresh payload
/// - `changed` -- same [`diff_key`] in both, and live bytes differ (or are
///   unreadable / missing)
/// - `removed` -- only in the record
///
/// A renamed main `.addon` surfaces as one `removed` plus one `added`.
#[derive(Debug, Default)]
pub(super) struct SetDiff {
    pub(super) added: Vec<LumaPayloadFile>,
    pub(super) changed: Vec<(PathBuf, Vec<u8>)>,
    pub(super) removed: Vec<PathBuf>,
}

/// Computes [`SetDiff`] from the record's tracked payload paths and owned
/// fresh payload files.
///
/// Consumes `files` so matching/new entries move into `changed`/`added`
/// without cloning bodies. Paths present in both sets are compared against
/// live bytes first so unchanged files are dropped.
///
/// `payload_dir` is the set-diff root (recorded add-on parent; may differ from
/// the executable `game_dir` when ReShade AddonPath is split).
pub(super) fn compute_diff(
    record: &InstalledAddon,
    payload_dir: &Path,
    files: Vec<LumaPayloadFile>,
    excluded_paths: &[PathBuf],
) -> SetDiff {
    let mut recorded_by_key: HashMap<String, PathBuf> =
        recorded_payload_keys(record, payload_dir, excluded_paths)
            .into_iter()
            .collect();

    let mut added = Vec::new();
    let mut changed = Vec::new();
    for file in files {
        if crate::addons::luma::dlss::is_dlss_relative_path(&file.relative_path) {
            continue;
        }
        let key = diff_key(&file.relative_path);
        if let Some(existing_path) = recorded_by_key.remove(&key) {
            if std::fs::read(&existing_path).ok().as_deref() != Some(file.bytes.as_slice()) {
                changed.push((existing_path, file.bytes));
            }
        } else {
            added.push(file);
        }
    }
    // Whatever's left in `recorded_by_key` is no longer in the fresh payload.
    let removed: Vec<PathBuf> = recorded_by_key.into_values().collect();

    SetDiff {
        added,
        changed,
        removed,
    }
}

/// [`Replacement`] values for the shared file-update path.
/// Identical live bytes are already dropped in [`compute_diff`].
pub(super) fn changed_replacements(changed: Vec<(PathBuf, Vec<u8>)>) -> Vec<Replacement> {
    changed
        .into_iter()
        .map(|(path, bytes)| Replacement {
            path,
            bytes,
            mtime: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{AddonKind, GameId};
    use tempfile::tempdir;

    use super::super::test_fixtures::{payload, payload_file};
    use super::*;

    #[test]
    fn diff_key_normalizes_separators_and_case() {
        assert_eq!(
            diff_key("Luma/Global/Copy_PS.hlsl"),
            "luma/global/copy_ps.hlsl"
        );
        assert_eq!(
            diff_key(r"Luma\Global\Copy_PS.hlsl"),
            "luma/global/copy_ps.hlsl"
        );
    }

    fn record_with_created(game_dir: &Path, relative_paths: &[&str]) -> InstalledAddon {
        let addon_path = game_dir.join(relative_paths[0]);
        let created: Vec<renderpilot_domain::PathRef> = relative_paths
            .iter()
            .map(|relative| {
                renderpilot_domain::PathRef::new(
                    game_dir.join(relative).to_string_lossy().into_owned(),
                )
                .expect("path")
            })
            .collect();
        InstalledAddon::from_parts(
            GameId::new("steam:403640").expect("id"),
            AddonKind::Luma,
            renderpilot_domain::PathRef::new(addon_path.to_string_lossy().into_owned())
                .expect("path"),
            None,
            created,
            Vec::new(),
            Vec::new(),
        )
        .expect("record")
    }

    #[test]
    fn compute_diff_skips_changed_when_on_disk_bytes_already_match() {
        let dir = tempdir().expect("tempdir");
        let addon = dir.path().join("Luma-Game.addon");
        let hlsl = dir.path().join("Luma").join("Global").join("A.hlsl");
        std::fs::create_dir_all(hlsl.parent().unwrap()).expect("mkdir");
        std::fs::write(&addon, b"addon").expect("write");
        std::fs::write(&hlsl, b"technique {}").expect("write");
        let record = record_with_created(dir.path(), &["Luma-Game.addon", "Luma/Global/A.hlsl"]);
        let fresh = payload(vec![
            payload_file("Luma-Game.addon", b"addon"),
            payload_file("Luma/Global/A.hlsl", b"technique {}"),
        ]);

        let diff = compute_diff(&record, dir.path(), fresh.files, &[]);

        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn compute_diff_records_changed_when_on_disk_bytes_differ() {
        let dir = tempdir().expect("tempdir");
        let addon = dir.path().join("Luma-Game.addon");
        std::fs::write(&addon, b"old-addon").expect("write");
        let record = record_with_created(dir.path(), &["Luma-Game.addon"]);
        let fresh = payload(vec![payload_file("Luma-Game.addon", b"new-addon")]);

        let diff = compute_diff(&record, dir.path(), fresh.files, &[]);

        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].1, b"new-addon");
    }

    #[test]
    fn compute_diff_detects_additions_and_removals() {
        let dir = tempdir().expect("tempdir");
        let record = record_with_created(
            dir.path(),
            &[
                "Luma-Game.addon",
                "Luma/Global/Old.hlsl",
                "Luma/Global/Keep.hlsl",
            ],
        );
        let fresh = payload(vec![
            payload_file("Luma-Game.addon", b"addon"),
            payload_file("Luma/Global/Keep.hlsl", b"technique {}"),
            payload_file("Luma/Global/New.hlsl", b"technique NEW {}"),
        ]);

        let diff = compute_diff(&record, dir.path(), fresh.files, &[]);

        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].relative_path, "Luma/Global/New.hlsl");
        assert_eq!(diff.changed.len(), 2); // addon + Keep.hlsl
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(
            diff.removed[0],
            dir.path().join("Luma").join("Global").join("Old.hlsl")
        );
    }

    #[test]
    fn compute_diff_treats_a_renamed_main_addon_as_remove_plus_add() {
        let dir = tempdir().expect("tempdir");
        let record = record_with_created(dir.path(), &["Luma-Old_Name.addon"]);
        let fresh = payload(vec![payload_file("Luma-New_Name.addon", b"addon-bytes")]);

        let diff = compute_diff(&record, dir.path(), fresh.files, &[]);

        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].relative_path, "Luma-New_Name.addon");
        assert!(diff.changed.is_empty());
        assert_eq!(diff.removed, vec![dir.path().join("Luma-Old_Name.addon")]);
    }

    #[test]
    fn compute_diff_matches_case_insensitively_via_diff_key() {
        let dir = tempdir().expect("tempdir");
        let record = record_with_created(dir.path(), &["Luma-Game.addon", "Luma/Global/A.hlsl"]);
        let fresh = payload(vec![
            payload_file("Luma-Game.addon", b"addon"),
            payload_file("luma/global/a.hlsl", b"technique {}"),
        ]);

        let diff = compute_diff(&record, dir.path(), fresh.files, &[]);

        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert_eq!(diff.changed.len(), 2);
    }

    #[test]
    fn compute_diff_ignores_excluded_managed_dependency_paths() {
        let dir = tempdir().expect("tempdir");
        let record = record_with_created(
            dir.path(),
            &["Luma-Game.addon", "D3D9.dll", "dgVoodoo.conf"],
        );
        let fresh = payload(vec![payload_file("Luma-Game.addon", b"addon")]);
        let excluded = vec![
            dir.path().join("D3D9.dll"),
            dir.path().join("dgVoodoo.conf"),
        ];

        let diff = compute_diff(&record, dir.path(), fresh.files, &excluded);

        assert!(diff.removed.is_empty());
        assert_eq!(diff.changed.len(), 1);
    }

    #[test]
    fn compute_diff_never_removes_adopted_reshade_ini_or_host_proxy() {
        // Empty-host adoption records ReShade.ini + proxy DLL in created_files.
        // Neither ships in the Luma release ZIP -- Full set-diff must not treat
        // them as removed payload (would delete the user's ini on repair/update).
        let dir = tempdir().expect("tempdir");
        let record =
            record_with_created(dir.path(), &["Luma-Game.addon", "dxgi.dll", "ReShade.ini"]);
        let fresh = payload(vec![payload_file("Luma-Game.addon", b"addon")]);

        let diff = compute_diff(&record, dir.path(), fresh.files, &[]);

        assert!(
            !diff
                .removed
                .iter()
                .any(|p| p.file_name().is_some_and(|n| n == "ReShade.ini")),
            "adopted ReShade.ini must not land in removed"
        );
        assert!(
            !diff
                .removed
                .iter()
                .any(|p| p.file_name().is_some_and(|n| n == "dxgi.dll")),
            "host proxy must not land in removed"
        );
        assert!(diff.removed.is_empty());
        assert_eq!(diff.changed.len(), 1);
    }

    #[test]
    fn compute_diff_excludes_managed_deps_via_same_path_not_raw_equality() {
        // Create real files so canonicalize-based same_path can succeed even when
        // the excluded PathBuf is rebuilt via join rather than the stored string.
        let dir = tempdir().expect("tempdir");
        let game_dir = dir.path();
        let addon = game_dir.join("Luma-Game.addon");
        let d3d9 = game_dir.join("D3D9.dll");
        let conf = game_dir.join("dgVoodoo.conf");
        std::fs::write(&addon, b"addon").expect("write");
        std::fs::write(&d3d9, b"wrapper").expect("write");
        std::fs::write(&conf, b"conf").expect("write");

        let record =
            record_with_created(game_dir, &["Luma-Game.addon", "D3D9.dll", "dgVoodoo.conf"]);
        let fresh = payload(vec![payload_file("Luma-Game.addon", b"addon")]);
        // Re-joined paths -- same location, potentially different PathBuf identity.
        let excluded = vec![game_dir.join("D3D9.dll"), game_dir.join("dgVoodoo.conf")];

        let diff = compute_diff(&record, game_dir, fresh.files, &excluded);

        assert!(
            diff.removed.is_empty(),
            "managed deps must never land in removed even when PathBuf forms differ"
        );
        assert!(
            !diff
                .removed
                .iter()
                .any(|p| p.file_name().is_some_and(|n| n == "D3D9.dll"))
        );
        // Live addon already matches the fresh payload -- no rewrite needed.
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn relative_under_tolerates_dot_component_form_drift() {
        let dir = tempdir().expect("tempdir");
        let game_dir = dir.path();
        let file = game_dir.join("Luma").join("A.hlsl");
        std::fs::create_dir_all(file.parent().unwrap()).expect("mkdir");
        std::fs::write(&file, b"x").expect("write");
        // root with a trailing `.` component vs clean path under it.
        let root_with_dot = game_dir.join(".");
        let relative = relative_under(&root_with_dot, &file).expect("relative");
        assert_eq!(diff_key(&relative.to_string_lossy()), "luma/a.hlsl");
    }

    #[test]
    fn surviving_payload_paths_keeps_paths_even_when_outside_root_for_keying() {
        let dir = tempdir().expect("tempdir");
        let game_dir = dir.path();
        let other = tempdir().expect("other");
        let outside = other.path().join("orphan.addon");
        std::fs::write(&outside, b"x").expect("write");
        let addon = game_dir.join("Luma-Game.addon");
        let created = vec![
            renderpilot_domain::PathRef::new(addon.to_string_lossy().into_owned()).expect("p"),
            renderpilot_domain::PathRef::new(outside.to_string_lossy().into_owned()).expect("p"),
        ];
        let record = InstalledAddon::from_parts(
            GameId::new("steam:403640").expect("id"),
            AddonKind::Luma,
            renderpilot_domain::PathRef::new(addon.to_string_lossy().into_owned()).expect("p"),
            None,
            created,
            Vec::new(),
            Vec::new(),
        )
        .expect("record");

        let survivors = surviving_payload_paths(&record, &[]);
        assert_eq!(survivors.len(), 2);
        // Diff keys exclude the outside path, but survivors retain it.
        let keys = recorded_payload_keys(&record, game_dir, &[]);
        assert_eq!(keys.len(), 1);
        assert!(keys[0].1.ends_with("Luma-Game.addon"));
    }
}
