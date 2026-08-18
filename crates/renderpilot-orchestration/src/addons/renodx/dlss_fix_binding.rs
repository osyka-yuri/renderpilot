//! One authoritative projection of RenoDX DLSS-Fix ownership.
//!
//! `created_files` is deletion authority; the tracked source is advisory
//! freshness/provenance. This resolver keeps every caller from independently
//! guessing that relationship or scanning a prefix elsewhere in a game folder.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use renderpilot_domain::{Architecture, InstalledAddon, TrackedSource, TrackedSourceRole};

use crate::addons::renodx::{arch_from_addon_file, source};
use crate::file_mutation::{V2DiskObservation, observe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DlssFixBindingState {
    None,
    SourceOnly,
    OwnedOnly,
    Bound,
    Invalid,
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct DlssFixBinding {
    pub(crate) state: DlssFixBindingState,
    pub(crate) target: PathBuf,
    pub(crate) arch: Option<Architecture>,
    pub(crate) observation: V2DiskObservation,
    pub(crate) source: Option<TrackedSource>,
    /// Any recorded path whose file name has the reserved DLSS-Fix stem. These
    /// paths are opaque to targeted companion mutations; generic RenoDX update
    /// only uses them to preserve durable-transaction isolation.
    pub(crate) isolation_paths: Vec<PathBuf>,
    /// Whether any companion path or source evidence is recorded, even when
    /// that evidence is malformed or incomplete.
    pub(crate) has_evidence: bool,
    main_payload_collision: bool,
}

impl DlssFixBinding {
    /// A legacy record cannot make the main RenoDX payload the companion target.
    #[must_use]
    pub(crate) fn main_payload_collides(&self) -> bool {
        self.main_payload_collision
    }
}

/// Resolves exactly one possible companion: the case-insensitive canonical name
/// beside the recorded main add-on with the main add-on's recorded architecture.
/// No directory scan is performed; a physical file is observed only at that exact
/// path and never authorizes deletion by itself.
pub(crate) fn resolve(record: &InstalledAddon) -> DlssFixBinding {
    let addon = Path::new(record.addon_file().as_str());
    let parent = addon.parent().map(Path::to_path_buf).unwrap_or_default();
    let arch = arch_from_addon_file(record.addon_file().as_str());
    let target = arch
        .map(|arch| parent.join(source::dlss_fix_file_name(arch)))
        .unwrap_or(parent.join("renodx-dlssfix.invalid"));
    let observation = observe(&target);

    let mut sources = record
        .tracked_sources()
        .iter()
        .filter(|source| source.role() == TrackedSourceRole::DlssFix);
    let source = sources.next().cloned();
    let duplicate_sources = sources.next().is_some();
    let owned_candidate_paths: Vec<PathBuf> = record
        .created_files()
        .iter()
        .filter_map(|path| {
            path.file_name()
                .filter(|name| source::is_dlss_fix_candidate_file_name(name))?;
            Some(PathBuf::from(path.as_str()))
        })
        .collect();
    let auxiliary_candidate_paths: Vec<PathBuf> = record
        .backed_up_files()
        .iter()
        .chain(record.managed_files().iter().map(|managed| managed.path()))
        .filter_map(|path| {
            path.file_name()
                .filter(|name| source::is_dlss_fix_candidate_file_name(name))?;
            Some(PathBuf::from(path.as_str()))
        })
        .collect();
    let mut isolation_keys = HashSet::new();
    let isolation_paths: Vec<PathBuf> = owned_candidate_paths
        .iter()
        .chain(&auxiliary_candidate_paths)
        .filter(|path| isolation_keys.insert(crate::paths::normalized_key(path)))
        .cloned()
        .collect();

    let exact_count = owned_candidate_paths
        .iter()
        .filter(|path| same_lexical_path(path, &target))
        .count();
    let wrong_managed_path = owned_candidate_paths
        .iter()
        .any(|path| !same_lexical_path(path, &target));
    let main_payload_collision = same_lexical_path(addon, &target);
    let invalid = arch.is_none()
        || duplicate_sources
        || exact_count > 1
        || wrong_managed_path
        || !auxiliary_candidate_paths.is_empty()
        || main_payload_collision
        || matches!(
            observation,
            V2DiskObservation::NonRegular | V2DiskObservation::Unreadable
        );
    let state = if invalid {
        DlssFixBindingState::Invalid
    } else {
        match (source.as_ref(), exact_count == 1) {
            (None, false) => DlssFixBindingState::None,
            (Some(_), false) => DlssFixBindingState::SourceOnly,
            (None, true) => DlssFixBindingState::OwnedOnly,
            (Some(_), true) => DlssFixBindingState::Bound,
        }
    };
    let has_evidence = !isolation_paths.is_empty() || source.is_some();
    DlssFixBinding {
        state,
        target,
        arch,
        observation,
        source,
        has_evidence,
        isolation_paths,
        main_payload_collision,
    }
}

fn same_lexical_path(left: &Path, right: &Path) -> bool {
    crate::paths::normalized_key(left) == crate::paths::normalized_key(right)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use renderpilot_domain::{AddonKind, GameId, PathRef};
    use tempfile::tempdir;

    use super::*;

    fn record(root: &Path) -> InstalledAddon {
        InstalledAddon::new(
            GameId::new("manual:binding").expect("game id"),
            AddonKind::RenoDx,
            PathRef::new(root.join("renodx-game.addon64").to_string_lossy()).expect("path"),
        )
    }

    fn source() -> TrackedSource {
        TrackedSource::new(
            TrackedSourceRole::DlssFix,
            "https://example.test/dlss",
            None,
            "old",
        )
    }

    #[test]
    fn case_insensitive_recorded_path_binds_the_canonical_architecture_target() {
        let dir = tempdir().expect("dir");
        let canonical_name = source::dlss_fix_file_name(Architecture::X64);
        let canonical_target = dir.path().join(&canonical_name);
        let recorded_target = dir.path().join(canonical_name.to_ascii_uppercase());
        fs::write(&canonical_target, b"companion").expect("companion");
        let record = record(dir.path())
            .with_created_file(PathRef::new(recorded_target.to_string_lossy()).expect("path"))
            .with_tracked_source(source());
        let binding = resolve(&record);
        assert_eq!(binding.state, DlssFixBindingState::Bound);
        assert_eq!(binding.target, canonical_target);
        assert_eq!(binding.arch, Some(Architecture::X64));
        assert_eq!(binding.isolation_paths, vec![recorded_target]);
        assert!(matches!(
            binding.observation,
            V2DiskObservation::Regular { .. }
        ));
    }

    #[test]
    fn source_only_and_owned_only_are_distinct_without_claiming_an_active_none_row() {
        let dir = tempdir().expect("dir");
        let target = dir.path().join("renodx-dlssfix.addon64");
        fs::write(&target, b"physical-only").expect("physical companion");
        assert_eq!(
            resolve(&record(dir.path())).state,
            DlssFixBindingState::None
        );
        assert_eq!(
            resolve(&record(dir.path()).with_tracked_source(source())).state,
            DlssFixBindingState::SourceOnly
        );
        assert_eq!(
            resolve(
                &record(dir.path())
                    .with_created_file(PathRef::new(target.to_string_lossy()).expect("path")),
            )
            .state,
            DlssFixBindingState::OwnedOnly
        );
    }

    #[test]
    fn duplicate_or_wrong_managed_projection_is_invalid() {
        let dir = tempdir().expect("dir");
        let target = dir.path().join("renodx-dlssfix.addon64");
        let record = record(dir.path())
            .with_created_file(PathRef::new(target.to_string_lossy()).expect("path"))
            .with_created_file(
                PathRef::new(dir.path().join("renodx-dlssfix.addon32").to_string_lossy())
                    .expect("path"),
            )
            .with_tracked_source(source());
        assert_eq!(resolve(&record).state, DlssFixBindingState::Invalid);
    }

    #[test]
    fn exposes_all_case_insensitive_candidates_without_granting_them_authority() {
        let dir = tempdir().expect("dir");
        let expected = dir.path().join("renodx-dlssfix.addon64");
        let wrong_arch = dir.path().join("RENODX-DLSSFIX.ADDON32");
        let record = record(dir.path())
            .with_created_file(PathRef::new(expected.to_string_lossy()).expect("path"))
            .with_created_file(PathRef::new(wrong_arch.to_string_lossy()).expect("path"));

        let binding = resolve(&record);

        assert!(binding.has_evidence);
        assert_eq!(binding.state, DlssFixBindingState::Invalid);
        assert_eq!(binding.isolation_paths, vec![expected, wrong_arch]);
    }

    #[test]
    fn auxiliary_candidate_is_isolated_but_never_grants_deletion_authority() {
        let dir = tempdir().expect("dir");
        let auxiliary = dir.path().join("renodx-dlssfix.legacy");
        let record = record(dir.path())
            .with_backed_up_file(PathRef::new(auxiliary.to_string_lossy()).expect("path"));

        let binding = resolve(&record);

        assert!(binding.has_evidence);
        assert_eq!(binding.state, DlssFixBindingState::Invalid);
        assert_eq!(binding.isolation_paths, vec![auxiliary]);
    }

    #[test]
    fn legacy_main_payload_collision_is_explicitly_invalid() {
        let dir = tempdir().expect("dir");
        let record = InstalledAddon::new(
            GameId::new("manual:binding-collision").expect("game id"),
            AddonKind::RenoDx,
            PathRef::new(dir.path().join("renodx-dlssfix.addon64").to_string_lossy())
                .expect("path"),
        );

        let binding = resolve(&record);

        assert!(binding.main_payload_collides());
        assert_eq!(binding.state, DlssFixBindingState::Invalid);
    }

    #[test]
    fn duplicate_source_or_a_nonregular_exact_target_is_invalid() {
        let dir = tempdir().expect("dir");
        let target = dir.path().join("renodx-dlssfix.addon64");
        fs::create_dir(&target).expect("nonregular target");
        let nonregular = record(dir.path())
            .with_created_file(PathRef::new(target.to_string_lossy()).expect("path"))
            .with_tracked_source(source());
        assert_eq!(resolve(&nonregular).state, DlssFixBindingState::Invalid);

        let duplicate_source = record(dir.path())
            .with_tracked_source(source())
            .with_tracked_source(TrackedSource::new(
                TrackedSourceRole::DlssFix,
                "https://example.test/other",
                None,
                "other",
            ));
        assert_eq!(
            resolve(&duplicate_source).state,
            DlssFixBindingState::Invalid
        );
    }
}
