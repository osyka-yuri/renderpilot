//! Change tracking for install plans (actions taken so rollback can undo them).
//! Separated for clarity and to keep the main engine module smaller.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use super::helpers;

/// Tracks the filesystem actions an install takes, as one ordered log so rollback
/// is the log replayed in reverse.
#[derive(Default)]
pub(crate) struct InstallChanges {
    pub(crate) actions: Vec<Action>,
}

/// One reversible filesystem action.
pub(crate) enum Action {
    /// A file written where none existed (removed on rollback).
    Created(PathBuf),
    /// A pre-existing file moved to `bak`, then overwritten at `path`.
    Replaced { path: PathBuf, bak: PathBuf },
    /// A file moved to `bak`, then the original deleted.
    Removed { path: PathBuf, bak: PathBuf },
    /// A file updated in-place with no on-disk `.bak`.
    Updated {
        path: PathBuf,
        original_bytes: Option<Vec<u8>>,
        whole_file_owned: bool,
    },
    /// A peer endpoint removed without creating a live rollback sidecar.
    PeerRemoved {
        path: PathBuf,
        parent: crate::fs::VerifiedDir,
        leaf: crate::fs::LeafName,
        original_bytes: Vec<u8>,
    },
    /// A peer endpoint created without an on-disk rollback sidecar.
    PeerCreated {
        path: PathBuf,
        parent: crate::fs::VerifiedDir,
        leaf: crate::fs::LeafName,
        expected_post: crate::fs::EntryObservation,
    },
    /// A peer endpoint replaced without an on-disk rollback sidecar.
    PeerReplaced {
        path: PathBuf,
        parent: crate::fs::VerifiedDir,
        leaf: crate::fs::LeafName,
        original_bytes: Vec<u8>,
        expected_post: crate::fs::EntryObservation,
    },
    /// A directory created for a nested file.  Peer directory creation keeps
    /// the native identity so rollback can remove only the exact object that
    /// was created; the ordinary legacy path uses `None` and retains its
    /// existing best-effort cleanup semantics.
    CreatedDir {
        path: PathBuf,
        identity: Option<String>,
    },
}

/// The outcome of a rollback.
pub(crate) struct UndoOutcome {
    pub(crate) failures: usize,
}

impl UndoOutcome {
    pub(crate) fn is_complete(&self) -> bool {
        self.failures == 0
    }
}

impl InstallChanges {
    pub(crate) fn undo(&self) -> UndoOutcome {
        let mut failures = 0;
        for action in self.actions.iter().rev() {
            match action {
                Action::Created(path) => {
                    if let Err(e) = fs::remove_file(path)
                        && e.kind() != std::io::ErrorKind::NotFound
                    {
                        tracing::warn!("rollback created {}: {}", path.display(), e);
                        failures += 1;
                    }
                }
                Action::Replaced { path, bak } => {
                    let _ = fs::remove_file(path);
                    if let Err(e) = fs::rename(bak, path) {
                        tracing::warn!("rollback restore {}: {}", path.display(), e);
                        failures += 1;
                    }
                }
                Action::Removed { path, bak } => {
                    if let Err(e) = fs::rename(bak, path) {
                        tracing::warn!("rollback restore removed {}: {}", path.display(), e);
                        failures += 1;
                    }
                }
                Action::Updated {
                    path,
                    original_bytes,
                    ..
                } => match original_bytes {
                    Some(bytes) => {
                        if let Err(e) = crate::fs::write_file_atomically(path, bytes) {
                            tracing::warn!("rollback update {}: {}", path.display(), e);
                            failures += 1;
                        }
                    }
                    None => {
                        let _ = fs::remove_file(path);
                    }
                },
                Action::PeerRemoved {
                    path,
                    parent,
                    leaf,
                    original_bytes,
                } => {
                    match parent.create_file_no_replace(
                        leaf,
                        original_bytes,
                        crate::fs::AuthorityMode::CooperativeSameUid,
                    ) {
                        Ok(crate::fs::CreateFileNoReplace::Created { .. }) => {}
                        Ok(crate::fs::CreateFileNoReplace::Occupied) => {
                            tracing::warn!(
                                "rollback peer removal {} found a foreign postimage",
                                path.display()
                            );
                            failures += 1;
                        }
                        Err(e) => {
                            tracing::warn!("rollback peer removal {}: {}", path.display(), e);
                            failures += 1;
                        }
                    }
                }
                Action::PeerCreated {
                    path,
                    parent,
                    leaf,
                    expected_post,
                } => {
                    if let Err(e) = parent.remove_exact(
                        leaf,
                        expected_post,
                        crate::fs::AuthorityMode::CooperativeSameUid,
                    ) {
                        tracing::warn!(
                            "rollback peer create {} found a foreign or missing postimage: {}",
                            path.display(),
                            e
                        );
                        failures += 1;
                    }
                }
                Action::PeerReplaced {
                    path,
                    parent,
                    leaf,
                    original_bytes,
                    expected_post,
                } => {
                    if let Err(e) =
                        parent.overwrite_regular_file(leaf, expected_post, original_bytes)
                    {
                        tracing::warn!("rollback peer replace {}: {}", path.display(), e);
                        failures += 1;
                    }
                }
                Action::CreatedDir {
                    path: dir,
                    identity,
                } => {
                    if let Some(identity) = identity {
                        let result = crate::fs::verified_parent(dir).and_then(|(parent, leaf)| {
                            parent.remove_empty_dir_exact(
                                &leaf,
                                identity,
                                crate::fs::AuthorityMode::CooperativeSameUid,
                            )
                        });
                        if let Err(error) = result {
                            tracing::warn!("rollback created directory {}: {error}", dir.display());
                            failures += 1;
                        }
                    } else if let Err(error) = helpers::remove_dir_if_empty(dir) {
                        tracing::warn!("rollback created {}: {}", dir.display(), error);
                        failures += 1;
                    }
                }
            }
            // failures counted above
        }
        UndoOutcome { failures }
    }

    pub(crate) fn sync_touched_dirs(&self) {
        let mut touched: HashSet<PathBuf> = HashSet::new();
        for action in &self.actions {
            match action {
                Action::Created(p)
                | Action::Replaced { path: p, .. }
                | Action::Removed { path: p, .. }
                | Action::Updated { path: p, .. } => {
                    if let Some(parent) = p.parent() {
                        touched.insert(parent.to_path_buf());
                    }
                }
                Action::PeerRemoved { parent, .. }
                | Action::PeerCreated { parent, .. }
                | Action::PeerReplaced { parent, .. } => {
                    let _ = parent.sync();
                }
                Action::CreatedDir { path: d, .. } => {
                    touched.insert(d.clone());
                }
            }
        }
        for dir in touched {
            crate::fs::sync_directory_best_effort(&dir);
        }
    }

    pub(crate) fn cleanup_remove_backups(&self) {
        for action in &self.actions {
            if let Action::Removed { bak, .. } = action {
                let _ = fs::remove_file(bak);
            }
        }
    }

    pub(crate) fn into_receipt(self) -> super::InstallReceipt {
        let mut created = Vec::new();
        let mut backed = Vec::new();
        for action in self.actions {
            match action {
                Action::Created(path)
                | Action::PeerCreated { path, .. }
                | Action::PeerReplaced { path, .. } => created.push(path),
                Action::Replaced { path, .. } => {
                    // path is needed in both lists — single intentional clone
                    backed.push(path.clone());
                    created.push(path);
                }
                Action::Updated {
                    path,
                    whole_file_owned,
                    original_bytes,
                } if whole_file_owned || original_bytes.is_none() => {
                    created.push(path);
                }
                // Removed files don't appear in the receipt; the caller knows
                // which file it asked to remove and updates its record directly.
                Action::PeerRemoved { .. }
                | Action::Removed { .. }
                | Action::Updated { .. }
                | Action::CreatedDir { .. } => {}
            }
        }
        super::InstallReceipt {
            created_files: created,
            backed_up_files: backed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_replace_rollback_rejects_a_foreign_file_with_the_same_digest() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("peer.dll");
        fs::write(&path, b"before").expect("original file");
        let (parent, leaf) = crate::fs::verified_parent(&path).expect("parent authority");
        let before = parent
            .observe_leaf(&leaf)
            .expect("observe original")
            .expect("original entry");
        let expected_post = parent
            .overwrite_regular_file(&leaf, &before, b"after")
            .expect("apply replacement");

        let foreign = root.path().join("foreign.dll");
        fs::write(&foreign, b"after").expect("foreign file");
        fs::remove_file(&path).expect("remove applied file");
        fs::rename(&foreign, &path).expect("install foreign file");
        let changes = InstallChanges {
            actions: vec![Action::PeerReplaced {
                path: path.clone(),
                parent,
                leaf,
                original_bytes: b"before".to_vec(),
                expected_post,
            }],
        };

        let outcome = changes.undo();

        assert_eq!(outcome.failures, 1);
        assert_eq!(fs::read(path).expect("foreign bytes"), b"after");
    }
}
