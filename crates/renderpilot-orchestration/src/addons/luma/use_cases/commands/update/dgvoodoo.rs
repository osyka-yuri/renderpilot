//! Managed dgVoodoo2 update/apply step.
//!
//! A Luma payload update and a ReShade-host update both have existing mechanics,
//! but dgVoodoo has a distinct ownership boundary. Only a record that owns every
//! current runtime DLL may update an outdated compatible runtime. Reused user
//! files, and partial/unsafe owned stacks, are carried through untouched.

use std::path::Path;

use renderpilot_domain::{InstalledAddon, TrackedSource, TrackedSourceRole};

use crate::ServiceError;
use crate::addons::engine;
use crate::addons::file_update::{
    OriginalFile, Replacement, ReplacementFailure, apply_replacements_with_outcome,
};
use crate::addons::luma::dgvoodoo as managed_dgvoodoo;
use crate::addons::luma::tracking;
use crate::addons::luma::use_cases::update_target::ResolvedUpdateTarget;
use crate::addons::records::source_with_role;
use crate::net::ProgressObserver;

/// Result of applying, or carrying over, the managed wrapper step.
pub(super) struct DgVoodooUpdateOutcome {
    pub(super) source: Option<TrackedSource>,
    pub(super) receipt: engine::InstallReceipt,
    pub(super) originals: Vec<OriginalFile>,
}

/// Managed dependency update prepared before the outer transaction mutates disk.
pub(super) struct PreparedDgVoodooUpdate {
    source: Option<TrackedSource>,
    game_dir: Option<std::path::PathBuf>,
    prepared: Option<managed_dgvoodoo::PreparedDgVoodoo>,
    merge_owned_config: bool,
}

impl PreparedDgVoodooUpdate {
    pub(super) fn unchanged(record: &InstalledAddon) -> Self {
        Self {
            source: source_with_role(record, TrackedSourceRole::DgVoodooWrapper).cloned(),
            game_dir: None,
            prepared: None,
            merge_owned_config: false,
        }
    }

    /// Whether this prepared plan will write managed dependency files on apply.
    #[must_use]
    pub(super) fn writes(&self) -> bool {
        self.prepared.is_some()
    }

    /// Absolute game directory the prepared write targets, when any.
    #[must_use]
    pub(super) fn write_game_dir(&self) -> Option<&Path> {
        self.game_dir.as_deref()
    }

    /// Exact local paths a prepared dependency update may write.
    pub(super) fn write_paths(&self) -> Vec<std::path::PathBuf> {
        let (Some(game_dir), Some(prepared)) = (&self.game_dir, &self.prepared) else {
            return Vec::new();
        };
        let mut paths: Vec<_> = prepared
            .files
            .iter()
            .map(|file| game_dir.join(&file.dest))
            .collect();
        if self.merge_owned_config {
            paths.push(game_dir.join(&prepared.config_file));
        }
        paths
    }

    pub(super) fn apply(self) -> Result<DgVoodooUpdateOutcome, ReplacementFailure> {
        let originals = match (self.game_dir, self.prepared) {
            (Some(game_dir), Some(prepared)) => {
                replace_existing(&game_dir, prepared, self.merge_owned_config)?
            }
            _ => Vec::new(),
        };
        // Paths that did not exist before this write become managed creates so
        // HostOnly/Full record rebuild can own them (defensive if install_map grows).
        let receipt = engine::InstallReceipt {
            created_files: originals
                .iter()
                .filter(|original| original.bytes.is_none())
                .map(|original| original.path.clone())
                .collect(),
            ..engine::InstallReceipt::default()
        };
        Ok(DgVoodooUpdateOutcome {
            source: self.source,
            receipt,
            originals,
        })
    }
}

pub(super) async fn prepare_if_needed(
    target: &ResolvedUpdateTarget,
    record: &InstalledAddon,
    progress: Option<&ProgressObserver<'_>>,
    force_full: bool,
) -> Result<PreparedDgVoodooUpdate, ServiceError> {
    let Some(requirement) = managed_dgvoodoo::requirement(target.external_requirement.as_ref())
    else {
        return Ok(PreparedDgVoodooUpdate::unchanged(record));
    };

    // Managed authority: wrapper source + non-empty owned map subset, and no
    // unowned existing map dest (user-reused stacks have no wrapper source).
    // A source alone never authorizes an update. Catalogue map growth that only
    // adds missing dests remains manageable via ownership sync.
    if !managed_dgvoodoo::record_can_manage_runtime(record, &target.game_dir, requirement) {
        return Ok(PreparedDgVoodooUpdate::unchanged(record));
    }
    let status = managed_dgvoodoo::owned_status(&target.game_dir, requirement);
    let needs_map_sync =
        managed_dgvoodoo::map_needs_ownership_sync(record, &target.game_dir, requirement);
    if !should_fetch_owned(status, force_full) && !needs_map_sync {
        return Ok(PreparedDgVoodooUpdate::unchanged(record));
    }

    let prepared = managed_dgvoodoo::fetch(requirement, progress).await?;
    let source = prepared.tracked_source();
    let config_owned = tracking::owns_path(record, &target.game_dir.join(&prepared.config_file));
    Ok(PreparedDgVoodooUpdate {
        source: Some(source),
        game_dir: Some(target.game_dir.clone()),
        prepared: Some(prepared),
        merge_owned_config: config_owned,
    })
}

/// Whether prepare should re-fetch the managed archive for this owned status.
///
/// Current: leave alone (including Repair — version-ok intact stack).
/// Outdated / Incomplete: always re-fetch and re-place.
/// Unknown: only Repair (`force_full`) best-effort reconverges; Update stays
/// conservative so a user-replaced PE is not clobbered by a passive probe.
#[must_use]
fn should_fetch_owned(status: managed_dgvoodoo::OwnedDgVoodooStatus, force_full: bool) -> bool {
    use managed_dgvoodoo::OwnedDgVoodooStatus as S;
    match status {
        S::Current => false,
        S::Outdated | S::Incomplete => true,
        S::Unknown => force_full,
    }
}

fn replace_existing(
    game_dir: &Path,
    mut prepared: managed_dgvoodoo::PreparedDgVoodoo,
    merge_owned_config: bool,
) -> Result<Vec<OriginalFile>, ReplacementFailure> {
    let files = std::mem::take(&mut prepared.files);
    let mut replacements: Vec<Replacement> = files
        .into_iter()
        .filter_map(|file| {
            let path = game_dir.join(&file.dest);
            (std::fs::read(&path).ok().as_deref() != Some(file.bytes.as_slice())).then_some({
                Replacement {
                    path,
                    bytes: file.bytes,
                    mtime: None,
                }
            })
        })
        .collect();

    if merge_owned_config {
        let config_path = game_dir.join(&prepared.config_file);
        let owned = std::fs::read_to_string(&config_path).ok();
        let base = owned.as_deref().unwrap_or(prepared.config_default.as_str());
        let merged = managed_dgvoodoo::merged_config(&prepared, base);
        if std::fs::read(&config_path).ok().as_deref() != Some(merged.as_bytes()) {
            replacements.push(Replacement {
                path: config_path,
                bytes: merged.into_bytes(),
                mtime: None,
            });
        }
    }

    apply_replacements_with_outcome(replacements)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::engine::IniSection;
    use crate::addons::luma::dgvoodoo::{PreparedDgVoodoo, PreparedDgVoodooFile};
    use renderpilot_domain::{AddonKind, GameId, PathRef};
    use tempfile::tempdir;

    fn prepared() -> PreparedDgVoodoo {
        PreparedDgVoodoo {
            version: "2.87.3".to_owned(),
            files: vec![PreparedDgVoodooFile {
                dest: "D3D9.dll".to_owned(),
                bytes: b"new-d3d9".to_vec(),
            }],
            config_file: "dgVoodoo.conf".to_owned(),
            config_default: "[General]\r\n".to_owned(),
            config_sections: vec![IniSection {
                name: "General".to_owned(),
                keys: vec![("OutputAPI".to_owned(), "d3d11_fl11_0".to_owned())],
            }],
            source_url: "https://example.test/dg.zip".to_owned(),
            source_etag: None,
            source_last_modified: None,
            archive_digest: "new-digest".to_owned(),
        }
    }

    fn record_with_source(game_dir: &Path, source: Option<TrackedSource>) -> InstalledAddon {
        let addon = game_dir.join("Luma-Test.addon");
        std::fs::write(&addon, b"addon").expect("addon");
        let mut sources = Vec::new();
        if let Some(source) = source {
            sources.push(source);
        }
        InstalledAddon::from_parts(
            GameId::new("steam:49520").expect("id"),
            AddonKind::Luma,
            PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
            None,
            vec![
                PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
                PathRef::new(game_dir.join("D3D9.dll").to_string_lossy().into_owned())
                    .expect("path"),
                PathRef::new(
                    game_dir
                        .join("dgVoodoo.conf")
                        .to_string_lossy()
                        .into_owned(),
                )
                .expect("path"),
            ],
            Vec::new(),
            sources,
        )
        .expect("record")
    }

    #[test]
    fn replace_existing_updates_managed_files_without_creating_backups() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("D3D9.dll"), b"old-d3d9").expect("write dll");
        std::fs::write(
            dir.path().join("dgVoodoo.conf"),
            "[General]\r\nOutputAPI=old\r\n",
        )
        .expect("write config");

        let originals = replace_existing(dir.path(), prepared(), true).expect("replace");

        assert_eq!(
            std::fs::read(dir.path().join("D3D9.dll")).unwrap(),
            b"new-d3d9"
        );
        let config = std::fs::read_to_string(dir.path().join("dgVoodoo.conf")).unwrap();
        assert!(config.contains("OutputAPI=d3d11_fl11_0"));
        assert!(!dir.path().join("D3D9.dll.bak").exists());
        assert!(!dir.path().join("dgVoodoo.conf.bak").exists());
        assert_eq!(originals.len(), 2);
    }

    #[test]
    fn replacement_never_merges_an_unowned_config() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("D3D9.dll"), b"old-d3d9").expect("write dll");
        std::fs::write(
            dir.path().join("dgVoodoo.conf"),
            "[General]\r\nOutputAPI=user-value\r\n",
        )
        .expect("write user config");

        let originals = replace_existing(dir.path(), prepared(), false).expect("replace dll only");

        assert_eq!(
            std::fs::read(dir.path().join("D3D9.dll")).unwrap(),
            b"new-d3d9"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("dgVoodoo.conf")).unwrap(),
            "[General]\r\nOutputAPI=user-value\r\n"
        );
        assert_eq!(originals.len(), 1);
    }

    #[test]
    fn dependency_apply_restores_earlier_files_when_a_later_write_fails() {
        let dir = tempdir().expect("tempdir");
        let dll = dir.path().join("D3D9.dll");
        std::fs::write(&dll, b"old-d3d9").expect("write dll");
        let mut prepared = prepared();
        prepared.files.push(PreparedDgVoodooFile {
            // Joining an empty destination resolves to the game directory,
            // which cannot be replaced as a file after D3D9.dll was written.
            dest: String::new(),
            bytes: b"invalid".to_vec(),
        });

        let failure = replace_existing(dir.path(), prepared, false)
            .expect_err("dependency write must fail on a directory target");

        assert!(failure.rollback_complete);
        assert_eq!(std::fs::read(&dll).expect("restored dll"), b"old-d3d9");
    }

    #[test]
    fn should_fetch_owned_matrix() {
        use managed_dgvoodoo::OwnedDgVoodooStatus as S;
        assert!(!should_fetch_owned(S::Current, false));
        assert!(!should_fetch_owned(S::Current, true));
        assert!(should_fetch_owned(S::Outdated, false));
        assert!(should_fetch_owned(S::Outdated, true));
        assert!(should_fetch_owned(S::Incomplete, false));
        assert!(should_fetch_owned(S::Incomplete, true));
        assert!(!should_fetch_owned(S::Unknown, false));
        assert!(should_fetch_owned(S::Unknown, true));
    }

    #[test]
    fn unchanged_carries_existing_source() {
        let dir = tempdir().expect("tempdir");
        let source = TrackedSource::new(
            TrackedSourceRole::DgVoodooWrapper,
            "https://example.test/dg.zip",
            None,
            "digest",
        );
        let record = record_with_source(dir.path(), Some(source.clone()));

        let outcome = PreparedDgVoodooUpdate::unchanged(&record)
            .apply()
            .expect("unchanged apply");

        assert_eq!(outcome.source, Some(source));
        assert!(outcome.originals.is_empty());
        assert!(outcome.receipt.created_files.is_empty());
    }

    #[test]
    fn apply_records_newly_created_paths_in_receipt() {
        let dir = tempdir().expect("tempdir");
        // Dest missing on disk → replace_existing creates it (bytes = None original).
        let prepared_update = PreparedDgVoodooUpdate {
            source: Some(TrackedSource::new(
                TrackedSourceRole::DgVoodooWrapper,
                "https://example.test/dg.zip",
                None,
                "digest",
            )),
            game_dir: Some(dir.path().to_path_buf()),
            prepared: Some(prepared()),
            merge_owned_config: false,
        };

        let outcome = prepared_update.apply().expect("apply creates missing dll");

        assert_eq!(
            std::fs::read(dir.path().join("D3D9.dll")).expect("dll"),
            b"new-d3d9"
        );
        assert_eq!(outcome.receipt.created_files.len(), 1);
        assert!(
            outcome
                .receipt
                .created_files
                .iter()
                .any(|path| path.ends_with("D3D9.dll"))
        );
        assert_eq!(outcome.originals.len(), 1);
        assert!(outcome.originals[0].bytes.is_none());
    }
}
