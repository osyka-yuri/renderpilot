//! Ordinary peer directory publication and cleanup.
//!
//! The plan in `ancestors` is pure.  This module is the only ordinary runtime
//! boundary that turns that plan into filesystem operations, and it does so
//! through the no-follow directory authority.  Retryable file transactions
//! deliberately do not use this module.

use std::cmp::Reverse;
use std::fs;
use std::path::{Path, PathBuf};

use crate::addons::engine::{Action, InstallChanges};
use crate::peer_mutation_executor::{
    PeerAncestorManifestEntry, PeerAncestorPlan, PeerPathObservation,
};
use crate::{ServiceError, failed};
use renderpilot_storage_sqlite::PeerRecoveryAncestor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParentState {
    Reachable,
    Missing,
}

/// Native authorities for directories created by the current peer ceremony.
///
/// These handles are kept past ancestor creation so endpoint creation can use
/// the exact directory object that was created.  A handle is duplicated when
/// more than one endpoint needs the same parent; that duplicates only the
/// native capability, not the directory contents.
#[derive(Default)]
pub(crate) struct PeerAncestorAuthorities {
    entries: Vec<PeerAncestorAuthority>,
}

struct PeerAncestorAuthority {
    path: PathBuf,
    directory: crate::fs::VerifiedDir,
}

impl PeerAncestorAuthorities {
    pub(crate) fn observe_created_endpoint(
        &self,
        endpoint: &Path,
    ) -> Result<Option<PeerPathObservation>, ServiceError> {
        let parent_path = endpoint
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or_else(|| {
                failed(format!(
                    "peer endpoint has no parent: {}",
                    endpoint.display()
                ))
            })?;
        let leaf = crate::fs::LeafName::from_path(endpoint)?;
        let Some(directory) = self
            .entries
            .iter()
            .find(|entry| same_path(&entry.path, parent_path))
            .map(|entry| &entry.directory)
        else {
            return Ok(None);
        };
        if directory.observe_leaf(&leaf)?.is_some() {
            return Err(failed(format!(
                "peer endpoint became occupied before creation: {}",
                endpoint.display()
            )));
        }
        Ok(Some(PeerPathObservation::Absent {
            parent: Some((directory.clone_capability()?, leaf)),
        }))
    }

    fn directory_for(&self, path: &Path) -> Option<&crate::fs::VerifiedDir> {
        self.entries
            .iter()
            .find(|entry| same_path(&entry.path, path))
            .map(|entry| &entry.directory)
    }

    fn push(&mut self, path: PathBuf, directory: crate::fs::VerifiedDir) {
        self.entries.push(PeerAncestorAuthority { path, directory });
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        let left = left.to_string_lossy();
        let right = right.to_string_lossy();
        renderpilot_domain::normalized_path_key(left.as_ref())
            == renderpilot_domain::normalized_path_key(right.as_ref())
    } else {
        left == right
    }
}

/// Rechecks every planned directory before the first create.  A missing
/// parent is safe (the endpoint is necessarily absent below it); an existing
/// parent is opened and inspected without following links or reparses.
pub(crate) fn verify_before_create(
    plan: &PeerAncestorPlan,
    roots: &[PathBuf],
) -> Result<(), ServiceError> {
    for ancestor in plan.ancestors() {
        let path = ancestor.path();
        let root = root_for(path, roots)?;
        if let ParentState::Reachable = parent_state(path, root)? {
            let (parent, leaf) = crate::fs::verified_parent(path)?;
            if parent.observe_leaf(&leaf)?.is_some() {
                return Err(failed(format!(
                    "peer ancestor became occupied before apply: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

/// Creates the exact missing ancestor set parent-first.  Every successful
/// creation is journaled immediately with its native identity, so a later
/// endpoint failure can only remove the directory that this ceremony created.
pub(crate) fn create(
    plan: &PeerAncestorPlan,
    roots: &[PathBuf],
    changes: &mut InstallChanges,
) -> Result<PeerAncestorAuthorities, ServiceError> {
    verify_before_create(plan, roots)?;
    let mut authorities = PeerAncestorAuthorities::default();
    for ancestor in plan.ancestors() {
        let path = ancestor.path();
        let leaf = crate::fs::LeafName::from_path(path)?;
        let parent_path = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or_else(|| failed(format!("peer ancestor has no parent: {}", path.display())))?;
        let created = if let Some(parent) = authorities.directory_for(parent_path) {
            create_directory(parent, &leaf, path)?
        } else {
            let root = root_for(path, roots)?;
            if parent_state(path, root)? != ParentState::Reachable {
                return Err(failed(format!(
                    "peer ancestor parent disappeared before create: {}",
                    path.display()
                )));
            }
            let parent = crate::fs::VerifiedDir::open(parent_path)?;
            create_directory(&parent, &leaf, path)?
        };
        changes.actions.push(Action::CreatedDir {
            path: path.to_path_buf(),
            identity: Some(created.identity().to_owned()),
        });
        authorities.push(path.to_path_buf(), created);
    }
    Ok(authorities)
}

fn create_directory(
    parent: &crate::fs::VerifiedDir,
    leaf: &crate::fs::LeafName,
    path: &Path,
) -> Result<crate::fs::VerifiedDir, ServiceError> {
    if parent.observe_leaf(leaf)?.is_some() {
        return Err(failed(format!(
            "peer ancestor became occupied before create: {}",
            path.display()
        )));
    }
    parent.create_directory_no_replace(leaf, crate::fs::AuthorityMode::CooperativeSameUid)
}

/// Cleans only the declared peer ancestors after endpoint restoration.  Any
/// residual or unsafe directory is deliberately preserved and logged; it is
/// not allowed to keep the durable row in Prepared state.
pub(crate) fn cleanup(entries: &[PeerAncestorManifestEntry], roots: &[PathBuf]) {
    cleanup_paths(entries.iter().map(PeerAncestorManifestEntry::path), roots);
}

/// Cleans the exact ancestor projection issued by storage for peer recovery.
/// It shares the same no-follow, best-effort implementation as forward
/// recovery and deliberately does not reconstruct manifest consumers.
pub(crate) fn cleanup_recovery(entries: &[PeerRecoveryAncestor], roots: &[PathBuf]) {
    cleanup_paths(entries.iter().map(PeerRecoveryAncestor::path), roots);
}

fn cleanup_paths<'a>(entries: impl IntoIterator<Item = &'a str>, roots: &[PathBuf]) {
    // The producer emits ancestors parent-first, but recovery must not make
    // that ordering an implicit trust boundary.  A torn or hand-repaired
    // manifest may reorder otherwise valid entries; removing by path depth
    // keeps every parent eligible only after all declared children have had a
    // best-effort removal attempt.
    let mut ordered = entries.into_iter().collect::<Vec<_>>();
    ordered.sort_by_key(|path| Reverse(Path::new(path).components().count()));

    for entry in ordered {
        let path = Path::new(entry);
        let root = match root_for(path, roots) {
            Ok(root) => root,
            Err(error) => {
                tracing::warn!("preserving peer ancestor {}: {error}", path.display());
                continue;
            }
        };
        match parent_state(path, root) {
            Ok(ParentState::Missing) => continue,
            Err(error) => {
                tracing::warn!("preserving peer ancestor {}: {error}", path.display());
                continue;
            }
            Ok(ParentState::Reachable) => {}
        }
        let (parent, leaf) = match crate::fs::verified_parent(path) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!("preserving peer ancestor {}: {error}", path.display());
                continue;
            }
        };
        let observed = match parent.observe_leaf(&leaf) {
            Ok(Some(observed)) => observed,
            Ok(None) => continue,
            Err(error) => {
                tracing::warn!("preserving peer ancestor {}: {error}", path.display());
                continue;
            }
        };
        if !observed.is_directory() {
            tracing::warn!("preserving non-directory peer ancestor {}", path.display());
            continue;
        }
        if let Err(error) = parent.remove_empty_dir_exact(
            &leaf,
            &observed.identity,
            crate::fs::AuthorityMode::CooperativeSameUid,
        ) {
            tracing::warn!("preserving peer ancestor {}: {error}", path.display());
        }
    }
}

fn root_for<'a>(path: &Path, roots: &'a [PathBuf]) -> Result<&'a Path, ServiceError> {
    roots
        .iter()
        .find(|root| crate::paths::is_within(path, root))
        .map(PathBuf::as_path)
        .ok_or_else(|| {
            failed(format!(
                "peer ancestor is outside authorized roots: {}",
                path.display()
            ))
        })
}

/// Classifies the parent chain without following links.  The walk continues
/// above a missing component so `/root/file/missing` is rejected as unsafe,
/// while `/root/missing/nested` is correctly treated as unreachable-but-safe.
fn parent_state(path: &Path, root: &Path) -> Result<ParentState, ServiceError> {
    let mut current = path
        .parent()
        .ok_or_else(|| failed(format!("peer ancestor has no parent: {}", path.display())))?;
    let mut missing = false;
    loop {
        if !crate::paths::is_within(current, root) {
            return Err(failed(format!(
                "peer ancestor parent is outside authorized root: {}",
                path.display()
            )));
        }
        match fs::symlink_metadata(current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(failed(format!(
                    "peer ancestor parent is a link or reparse point: {}",
                    current.display()
                )));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(failed(format!(
                    "peer ancestor parent is not a directory: {}",
                    current.display()
                )));
            }
            Ok(_) => {
                return Ok(if missing {
                    ParentState::Missing
                } else {
                    ParentState::Reachable
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing = true;
            }
            Err(error) => {
                return Err(failed(format!(
                    "failed to inspect peer ancestor parent {}: {error}",
                    current.display()
                )));
            }
        }
        current = current.parent().ok_or_else(|| {
            failed(format!(
                "peer ancestor parent has no root: {}",
                path.display()
            ))
        })?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_domain::{PathRef, PeerEndpointRole, Sha256Hash};

    use crate::peer_mutation_executor::{
        EndpointExpectation, EndpointObservation, EndpointPostcondition, ExactEndpoint,
        ExactEndpointProgram,
    };

    fn plan(root: &Path) -> PeerAncestorPlan {
        let endpoint = PathRef::new(
            root.join("nested/deeper/peer.dll")
                .to_string_lossy()
                .replace('\\', "/"),
        )
        .expect("endpoint");
        let digest = Sha256Hash::new("a".repeat(64)).expect("digest");
        let program = ExactEndpointProgram::new(vec![ExactEndpoint::new(
            endpoint,
            PeerEndpointRole::Disjoint,
            EndpointExpectation::Absent,
            EndpointPostcondition::File(digest),
        )])
        .expect("program");
        PeerAncestorPlan::derive(&program, &[EndpointObservation::Absent], &[root.to_owned()])
            .expect("plan")
    }

    #[test]
    fn creates_nested_ancestors_parent_first_and_undoes_exact_identity() {
        let root = tempfile::tempdir().expect("root");
        let plan = plan(root.path());
        let mut changes = InstallChanges::default();

        let authorities = create(&plan, &[root.path().to_owned()], &mut changes).expect("create");

        assert!(root.path().join("nested/deeper").is_dir());
        assert_eq!(changes.actions.len(), 2);

        let endpoint_parent = root.path().join("nested/deeper");
        let PeerPathObservation::Absent {
            parent: Some((first_parent, first_leaf)),
        } = authorities
            .observe_created_endpoint(&endpoint_parent.join("first.dll"))
            .expect("first retained parent")
            .expect("created parent authority")
        else {
            panic!("created parent observation was not absent");
        };
        let PeerPathObservation::Absent {
            parent: Some((second_parent, second_leaf)),
        } = authorities
            .observe_created_endpoint(&endpoint_parent.join("second.dll"))
            .expect("second retained parent")
            .expect("created parent authority")
        else {
            panic!("created parent observation was not absent");
        };
        assert!(matches!(
            first_parent.create_file_no_replace(
                &first_leaf,
                b"first",
                crate::fs::AuthorityMode::CooperativeSameUid,
            ),
            Ok(crate::fs::CreateFileNoReplace::Created { .. })
        ));
        assert!(matches!(
            second_parent.create_file_no_replace(
                &second_leaf,
                b"second",
                crate::fs::AuthorityMode::CooperativeSameUid,
            ),
            Ok(crate::fs::CreateFileNoReplace::Created { .. })
        ));
        assert_eq!(
            fs::read(endpoint_parent.join("first.dll")).expect("first"),
            b"first"
        );
        assert_eq!(
            fs::read(endpoint_parent.join("second.dll")).expect("second"),
            b"second"
        );
        fs::remove_file(endpoint_parent.join("first.dll")).expect("remove first");
        fs::remove_file(endpoint_parent.join("second.dll")).expect("remove second");
        drop(first_parent);
        drop(second_parent);
        drop(authorities);
        assert!(changes.undo().is_complete());
        assert!(!root.path().join("nested").exists());
    }

    #[test]
    fn occupied_ancestor_is_rejected_before_any_directory_is_created() {
        let root = tempfile::tempdir().expect("root");
        let plan = plan(root.path());
        fs::write(root.path().join("nested"), b"foreign").expect("foreign entry");
        let mut changes = InstallChanges::default();

        assert!(create(&plan, &[root.path().to_owned()], &mut changes).is_err());
        assert!(changes.actions.is_empty());
        assert!(root.path().join("nested").is_file());
    }

    #[test]
    fn cleanup_removes_empty_ancestors_but_preserves_nonempty_residuals() {
        let root = tempfile::tempdir().expect("root");
        let plan = plan(root.path());
        let mut changes = InstallChanges::default();
        create(&plan, &[root.path().to_owned()], &mut changes).expect("create");
        fs::write(root.path().join("nested/deeper/foreign"), b"foreign").expect("foreign");

        cleanup(&plan.manifest_entries(), &[root.path().to_owned()]);

        assert!(root.path().join("nested/deeper").exists());
        assert!(root.path().join("nested").exists());
        fs::remove_file(root.path().join("nested/deeper/foreign")).expect("remove foreign");
        cleanup(&plan.manifest_entries(), &[root.path().to_owned()]);
        assert!(!root.path().join("nested").exists());
    }

    #[test]
    fn cleanup_orders_by_depth_even_when_manifest_entries_are_reversed() {
        let root = tempfile::tempdir().expect("root");
        let plan = plan(root.path());
        let mut changes = InstallChanges::default();
        create(&plan, &[root.path().to_owned()], &mut changes).expect("create");

        let mut entries = plan.manifest_entries();
        entries.reverse();
        cleanup(&entries, &[root.path().to_owned()]);

        assert!(!root.path().join("nested/deeper").exists());
        assert!(!root.path().join("nested").exists());
    }

    #[test]
    fn cleanup_preserves_non_directory_residual() {
        let root = tempfile::tempdir().expect("root");
        let plan = plan(root.path());
        let mut changes = InstallChanges::default();
        create(&plan, &[root.path().to_owned()], &mut changes).expect("create");
        fs::remove_dir(root.path().join("nested/deeper")).expect("remove child directory");
        fs::write(root.path().join("nested/deeper"), b"foreign").expect("foreign file");

        cleanup(&plan.manifest_entries(), &[root.path().to_owned()]);

        assert!(root.path().join("nested/deeper").is_file());
        assert!(root.path().join("nested").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_preserves_reparse_residual() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("root");
        let outside = tempfile::tempdir().expect("outside");
        let plan = plan(root.path());
        let mut changes = InstallChanges::default();
        create(&plan, &[root.path().to_owned()], &mut changes).expect("create");
        fs::remove_dir_all(root.path().join("nested/deeper")).expect("remove child directory");
        symlink(outside.path(), root.path().join("nested/deeper")).expect("foreign link");

        cleanup(&plan.manifest_entries(), &[root.path().to_owned()]);

        assert!(root.path().join("nested/deeper").read_link().is_ok());
        assert!(root.path().join("nested").is_dir());
    }
}
