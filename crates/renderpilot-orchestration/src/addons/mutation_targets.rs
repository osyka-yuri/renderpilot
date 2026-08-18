//! Shared live/sidecar path sets for durable game-file transactions.

use std::path::{Path, PathBuf};

use renderpilot_domain::InstalledAddon;

use crate::ServiceError;
use crate::file_mutation::MutationScope;

/// Canonical directories and every live/sidecar path a command may touch.
#[derive(Debug, Clone)]
pub(crate) struct MutationTargets {
    pub(crate) roots: Vec<PathBuf>,
    pub(crate) paths: Vec<PathBuf>,
}

/// Resolved form of [`MutationTargets`] for crash-recoverable work.
///
/// Install/update keep using strict [`MutationTargets::into_scope_and_paths`].
/// Uninstall/orphan cleanup uses [`MutationTargets::resolve_workset`] so a
/// completely unreachable game folder still clears metadata.
#[derive(Debug, Clone)]
pub(crate) enum DurableWorkset {
    /// At least one root is reachable; snapshot only paths under those roots.
    Files {
        scope: MutationScope,
        paths: Vec<PathBuf>,
    },
    /// No declared root has an existing filesystem ancestor. File restore is a
    /// no-op; the feature commit is metadata-only (no file-mutation row).
    MetadataOnly,
}

impl MutationTargets {
    /// Preferred constructor for ad-hoc path sets: live paths are expanded with
    /// classic `.bak` sidecars and every path parent (plus optional extra roots)
    /// becomes an authorized mutation root.
    pub(crate) fn for_live_paths(
        extra_roots: impl IntoIterator<Item = PathBuf>,
        live_paths: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        Self::from_preexpanded_paths(extra_roots, crate::fs::expand_with_sidecars(live_paths))
    }

    /// Preferred constructor for record-driven work: record live/sidecar set
    /// plus extras, with the add-on parent and optional extra roots authorized.
    ///
    /// `extra_live_paths` are treated as live paths and expanded with classic
    /// `.bak` sidecars. The record's own live/sidecar set is already expanded.
    pub(crate) fn for_record(
        record: &InstalledAddon,
        extra_roots: impl IntoIterator<Item = PathBuf>,
        extra_live_paths: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        Self::for_record_excluding(record, extra_roots, extra_live_paths, std::iter::empty())
    }

    /// Record-driven target set that excludes exact live paths and their
    /// sidecars. Used where one tracked projection must remain untouched by a
    /// broader durable mutation owned by the same record.
    pub(crate) fn for_record_excluding(
        record: &InstalledAddon,
        extra_roots: impl IntoIterator<Item = PathBuf>,
        extra_live_paths: impl IntoIterator<Item = PathBuf>,
        excluded_live_paths: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        let mut roots: Vec<PathBuf> = extra_roots.into_iter().collect();
        if let Some(parent) = Path::new(record.addon_file().as_str())
            .parent()
            .map(Path::to_path_buf)
        {
            roots.push(parent);
        }
        let mut paths = crate::addons::records::record_live_and_sidecar_paths(record);
        paths.extend(crate::fs::expand_with_sidecars(extra_live_paths));
        let excluded: std::collections::HashSet<String> =
            crate::fs::expand_with_sidecars(excluded_live_paths)
                .into_iter()
                .map(|path| crate::paths::normalized_key(&path))
                .collect();
        paths.retain(|path| !excluded.contains(&crate::paths::normalized_key(path)));
        Self::from_preexpanded_paths(roots, paths)
    }

    /// Builds targets from explicit roots and live paths, expanding each live
    /// path with its classic `.bak` sidecar. Unlike [`Self::for_live_paths`],
    /// parent directories of live paths are not auto-added as roots.
    pub(crate) fn from_roots_and_live_paths(
        roots: impl IntoIterator<Item = PathBuf>,
        live_paths: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        Self {
            roots: roots.into_iter().collect(),
            paths: crate::fs::expand_with_sidecars(live_paths),
        }
    }

    /// Builds targets from paths that are already expanded (live + sidecars).
    fn from_preexpanded_paths(
        extra_roots: impl IntoIterator<Item = PathBuf>,
        paths: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        let paths: Vec<PathBuf> = paths.into_iter().collect();
        let mut roots: Vec<PathBuf> = extra_roots.into_iter().collect();
        for path in &paths {
            if let Some(parent) = path.parent() {
                roots.push(parent.to_path_buf());
            }
        }
        Self { roots, paths }
    }

    /// Consumes into `(scope, paths)` for install/update/swap, requiring every
    /// root to be reachable.
    pub(crate) fn into_scope_and_paths(
        self,
    ) -> Result<(MutationScope, Vec<PathBuf>), ServiceError> {
        let paths = self.paths;
        let scope = MutationScope::new(self.roots)?;
        Ok((scope, paths))
    }

    /// Resolves targets for uninstall/orphan cleanup: keeps only reachable roots
    /// and paths under them, or [`DurableWorkset::MetadataOnly`] when nothing is
    /// reachable.
    pub(crate) fn resolve_workset(self) -> Result<DurableWorkset, ServiceError> {
        let Some(scope) = MutationScope::try_from_reachable_roots(self.roots)? else {
            return Ok(DurableWorkset::MetadataOnly);
        };
        let paths = self
            .paths
            .into_iter()
            .filter(|path| scope.contains_reachable(path))
            .collect();
        Ok(DurableWorkset::Files { scope, paths })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_live_paths_expands_sidecars_and_parent_roots() {
        let live = PathBuf::from(r"C:\Games\Title\nvngx_dlss.dll");
        let targets =
            MutationTargets::for_live_paths([PathBuf::from(r"C:\Games\Title")], [live.clone()]);
        assert!(targets.paths.iter().any(|p| p == &live));
        assert!(
            targets
                .paths
                .iter()
                .any(|p| p.extension().is_some_and(|e| e == "bak"))
        );
        assert!(
            targets
                .roots
                .iter()
                .any(|r| r == Path::new(r"C:\Games\Title"))
        );
    }

    #[test]
    fn resolve_workset_is_metadata_only_when_no_root_has_an_ancestor() {
        let targets = MutationTargets::for_live_paths(
            [PathBuf::from("Z:/renderpilot-no-such-volume/Game")],
            [PathBuf::from(
                "Z:/renderpilot-no-such-volume/Game/addon.addon64",
            )],
        );
        match targets.resolve_workset().expect("resolve") {
            DurableWorkset::MetadataOnly => {}
            DurableWorkset::Files { .. } => panic!("expected metadata-only for unreachable roots"),
        }
    }

    #[test]
    fn resolve_workset_keeps_reachable_temp_roots() {
        let dir = tempfile::tempdir().expect("tempdir");
        let live = dir.path().join("addon.addon64");
        let targets = MutationTargets::for_live_paths([dir.path().to_path_buf()], [live.clone()]);
        match targets.resolve_workset().expect("resolve") {
            DurableWorkset::Files { scope, paths } => {
                assert!(scope.contains_reachable(&live));
                assert!(paths.iter().any(|p| p == &live));
            }
            DurableWorkset::MetadataOnly => panic!("tempdir root must be reachable"),
        }
    }
}
