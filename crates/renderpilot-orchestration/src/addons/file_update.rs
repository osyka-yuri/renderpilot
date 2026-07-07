//! Shared in-place file-update machinery for addon update flows.
//!
//! A file an update flow (add-on/host/DLSS-Fix update, ReShade channel switch)
//! overwrites in place with **no on-disk backup** — the artifact is a rolling
//! upstream snapshot or an official redistributable already PE-sanity-checked on
//! the way in, so nothing about the previous bytes is worth preserving for manual
//! recovery. [`engine::replace_file`] (temp+rename) makes each individual write
//! crash-safe on its own; no separate engine sentinel is needed for this in-place
//! path. What *is* needed is a uniform way to put every touched file back if a
//! *later* step in the same flow fails before the result is durably persisted —
//! that's what [`apply_replacements`]/[`restore_originals`]/
//! [`restore_originals_best_effort`] provide.

use std::path::PathBuf;

use super::errors::failed;
use crate::ServiceError;
use crate::addons::engine;

/// A file to overwrite in place with `bytes`, optionally stamping `mtime`.
#[derive(Debug)]
pub(crate) struct Replacement {
    pub(crate) path: PathBuf,
    pub(crate) bytes: Vec<u8>,
    pub(crate) mtime: Option<String>,
}

/// A [`Replacement`]'s pre-write state, captured so a later failure — anywhere
/// before the flow's result is durably persisted — can restore every file it
/// touched, in one uniform pass via [`restore_originals`]/
/// [`restore_originals_best_effort`]. `None` when the path didn't exist before
/// the write (restored by deleting it again).
#[derive(Debug)]
pub(crate) struct OriginalFile {
    pub(crate) path: PathBuf,
    pub(crate) bytes: Option<Vec<u8>>,
}

/// Writes every `replacement` in place, capturing each file's pre-write state
/// (existing bytes, or `None` if it didn't exist). On the first failure,
/// everything written so far *by this call* is rolled back before the error is
/// returned. A caller composing this with further writes (e.g. a host install)
/// is responsible for rolling those back too, and for rolling this call's
/// successful writes back again if something *after* this call fails.
pub(crate) fn apply_replacements(
    replacements: Vec<Replacement>,
) -> Result<Vec<OriginalFile>, ServiceError> {
    let mut originals = Vec::with_capacity(replacements.len());

    for replacement in replacements {
        let original_bytes = if replacement.path.is_file() {
            match crate::fs::read_file(&replacement.path) {
                Ok(bytes) => Some(bytes),
                Err(error) => {
                    restore_originals_best_effort(&originals);
                    return Err(error);
                }
            }
        } else {
            None
        };

        if let Err(error) = engine::replace_file(&replacement.path, &replacement.bytes) {
            restore_originals_best_effort(&originals);
            return Err(error);
        }
        crate::fs::stamp_mtime_best_effort(&replacement.path, replacement.mtime.as_deref(), None);

        originals.push(OriginalFile {
            path: replacement.path,
            bytes: original_bytes,
        });
    }

    Ok(originals)
}

/// Restores every file in `originals` to its pre-write state, in reverse order,
/// failing with an error naming how many could not be restored. Used when a
/// failure must be reported as a whole — e.g. a DB persistence failure, where
/// the caller needs to know whether disk state might not match what was
/// recorded.
pub(crate) fn restore_originals(originals: &[OriginalFile]) -> Result<(), ServiceError> {
    let failures = restore_originals_inner(originals);
    if failures == 0 {
        Ok(())
    } else {
        Err(failed(format!(
            "failed to restore {failures} updated file(s)"
        )))
    }
}

/// Same as [`restore_originals`], but only logs a failure rather than
/// returning one — used when the caller is already on an error path reporting
/// a different, primary failure.
pub(crate) fn restore_originals_best_effort(originals: &[OriginalFile]) {
    let failures = restore_originals_inner(originals);
    if failures > 0 {
        log::warn!("addon update rollback failed to restore {failures} file(s)");
    }
}

fn restore_originals_inner(originals: &[OriginalFile]) -> usize {
    let mut failures = 0;
    for original in originals.iter().rev() {
        let result = match &original.bytes {
            Some(bytes) => engine::replace_file(&original.path, bytes),
            None => crate::fs::remove_file_if_exists(&original.path),
        };
        if let Err(error) = result {
            log::warn!(
                "addon update rollback: failed to restore `{}`: {error}",
                original.path.display()
            );
            failures += 1;
        }
    }
    failures
}

/// Builds the error to return after a DB persistence failure follows one or
/// more on-disk mutations that were then rolled back. If every rollback
/// attempt in `restore_results` succeeded, `db_error` is returned unchanged —
/// disk state matches what it was before the operation, so there's nothing
/// else to report. If any rollback failed, the returned error names both
/// facts, so the caller isn't left thinking a clean DB error means a clean
/// disk state when files may actually be stranded in the new, unrecorded
/// state.
pub(crate) fn persistence_failure_error(
    db_error: ServiceError,
    restore_results: &[Result<(), ServiceError>],
) -> ServiceError {
    let restore_failures: Vec<String> = restore_results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .map(ToString::to_string)
        .collect();
    if restore_failures.is_empty() {
        return db_error;
    }
    failed(format!(
        "failed to save the update ({db_error}), and the on-disk rollback also failed \
         ({}); the game's files may not match its recorded state",
        restore_failures.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use std::path::Path;

    use super::*;
    use tempfile::tempdir;

    fn read(path: &Path) -> Vec<u8> {
        std::fs::read(path).expect("file should exist")
    }

    #[test]
    fn apply_replacements_captures_originals_and_writes_new_bytes() {
        let dir = tempdir().expect("tempdir");
        let existing = dir.path().join("addon-cp2077.dat");
        std::fs::write(&existing, b"old-addon").expect("write");
        let fresh = dir.path().join("addon-companion.dat");

        let originals = apply_replacements(vec![
            Replacement {
                path: existing.clone(),
                bytes: b"new-addon".to_vec(),
                mtime: None,
            },
            Replacement {
                path: fresh.clone(),
                bytes: b"new-companion".to_vec(),
                mtime: None,
            },
        ])
        .expect("apply");

        assert_eq!(read(&existing), b"new-addon");
        assert_eq!(read(&fresh), b"new-companion");
        assert_eq!(originals[0].bytes.as_deref(), Some(&b"old-addon"[..]));
        assert_eq!(originals[1].bytes, None);
    }

    #[test]
    fn apply_replacements_rolls_back_everything_written_so_far_on_a_mid_loop_failure() {
        let dir = tempdir().expect("tempdir");
        let existing = dir.path().join("addon-cp2077.dat");
        std::fs::write(&existing, b"old-addon").expect("write");
        // A path with no parent directory at all can never be written to —
        // forces `engine::replace_file`'s second call to fail.
        let unwritable = PathBuf::from("");

        let error = apply_replacements(vec![
            Replacement {
                path: existing.clone(),
                bytes: b"new-addon".to_vec(),
                mtime: None,
            },
            Replacement {
                path: unwritable,
                bytes: b"never-written".to_vec(),
                mtime: None,
            },
        ])
        .expect_err("second replacement should fail");
        assert_matches!(error, ServiceError::CommandFailed(_));

        // The first replacement's write is rolled back to its pre-call bytes.
        assert_eq!(read(&existing), b"old-addon");
    }

    #[test]
    fn restore_originals_restores_bytes_or_deletes_when_none_existed() {
        let dir = tempdir().expect("tempdir");
        let existed_before = dir.path().join("host.dll");
        let created_fresh = dir.path().join("addon-cp2077.dat");
        std::fs::write(&existed_before, b"new-host").expect("write");
        std::fs::write(&created_fresh, b"new-addon").expect("write");

        restore_originals(&[
            OriginalFile {
                path: existed_before.clone(),
                bytes: Some(b"old-host".to_vec()),
            },
            OriginalFile {
                path: created_fresh.clone(),
                bytes: None,
            },
        ])
        .expect("restore");

        assert_eq!(read(&existed_before), b"old-host");
        assert!(!created_fresh.exists());
    }

    #[test]
    fn persistence_failure_error_returns_bare_db_error_when_rollback_succeeds() {
        let db_error = failed("db unavailable".to_owned());

        let error = persistence_failure_error(db_error, &[Ok(()), Ok(())]);

        match error {
            ServiceError::CommandFailed(message) => assert_eq!(message, "db unavailable"),
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }

    #[test]
    fn persistence_failure_error_combines_db_and_rollback_failures() {
        let db_error = failed("db unavailable".to_owned());
        let rollback_error = failed("disk full".to_owned());

        let error = persistence_failure_error(db_error, &[Ok(()), Err(rollback_error)]);

        match error {
            ServiceError::CommandFailed(message) => {
                assert!(message.contains("db unavailable"), "{message}");
                assert!(message.contains("disk full"), "{message}");
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }
}
