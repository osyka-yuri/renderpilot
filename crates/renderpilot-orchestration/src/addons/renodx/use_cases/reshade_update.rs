/// Shared helpers for ReShade host update queries and commands.
use std::path::{Path, PathBuf};

use renderpilot_application::GameRepository;
use renderpilot_domain::{Architecture, GameId, InstalledAddon, TrackedSource, TrackedSourceRole};

use crate::{Context, ServiceError};

use crate::addons::engine;
use crate::addons::renodx::channel;
use crate::addons::renodx::errors;
use crate::addons::renodx::facts::{analyze_game, install_target_dir};
use crate::addons::renodx::host_policy;
use crate::addons::renodx::matcher::{RenoDxResolution, resolve};
use crate::addons::renodx::reshade::ReshadeHostAction;
use crate::addons::renodx::source;
use crate::addons::renodx::types::{RenoDxManifest, ReshadeChannel};

/// Resolves the tracked source with the given role, if the install recorded one.
pub(crate) fn source_with_role(
    record: &InstalledAddon,
    role: TrackedSourceRole,
) -> Option<&TrackedSource> {
    record
        .tracked_sources()
        .iter()
        .find(|source| source.role() == role)
}

/// A cosmetic fetch/log label for the add-on (the file name identifies the title).
pub(crate) fn addon_label(record: &InstalledAddon) -> &str {
    Path::new(record.addon_file().as_str())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("RenoDX add-on")
}

/// Recorded ReShade channel, including legacy URL-derived records.
pub(crate) fn recorded_reshade_channel(record: &InstalledAddon) -> Option<ReshadeChannel> {
    record
        .reshade_channel()
        .and_then(|c| ReshadeChannel::parse_recorded(Some(c)).into_parsed())
        .or_else(|| {
            channel::installed_channel(record)
                .ok()
                .flatten()
                .and_then(|c| c.into_parsed())
        })
}

/// Resolved ReShade host update target for proxy installs.
pub(crate) struct HostUpdateTarget {
    /// Game directory holding the proxy host.
    pub(crate) game_dir: PathBuf,
    /// Proxy slot file name.
    pub(crate) slot: String,
    /// ReShade host architecture.
    pub(crate) arch: Architecture,
    /// Host policy action required by current disk state.
    pub(crate) action: ReshadeHostAction,
    /// Whether the host policy found a conflict.
    pub(crate) conflict: bool,
    /// ReShade source for the requested channel.
    pub(crate) source: source::ReshadeSource,
    /// Requested/effective channel.
    pub(crate) channel: ReshadeChannel,
    /// Existing target path for the host.
    pub(crate) target_path: PathBuf,
}

/// Resolves the target ReShade host and source for a proxy-host update. `Ok(None)`
/// both when there's nothing to resolve (game/title/host unresolvable) *and* when
/// the active slot is a recognized custom build (e.g. GShade, see
/// [`host_policy::HostAssessment::is_known_custom_build`]) — RenoDX never checks
/// it for updates or replaces it, so every caller gets that guarantee for free
/// without checking for it itself.
pub(crate) fn resolve_host_update_target(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    channel: ReshadeChannel,
) -> Result<Option<HostUpdateTarget>, ServiceError> {
    let Some(game) = context.storage().find_game(game_id)? else {
        return Ok(None);
    };
    let override_path = crate::nvapi::resolve::stored_override_path(context, game_id.as_str())
        .ok()
        .flatten();
    let analysis = analyze_game(&game, override_path.as_deref());
    let resolution = resolve(manifest, &analysis.facts);
    let (arch, proxy_dll_name) = match resolution {
        RenoDxResolution::Installable(plan) => (plan.arch, plan.proxy_dll_name.clone()),
        RenoDxResolution::External {
            file_install: Some(plan),
            ..
        } => (plan.arch, plan.proxy_dll_name.clone()),
        _ => return Ok(None),
    };
    let game_dir = install_target_dir(&analysis)?;
    let assessment = host_policy::assess(&game_dir, &proxy_dll_name);
    if assessment.is_known_custom_build() {
        return Ok(None);
    }
    let source = source::require_reshade_source(&manifest.reshade, channel, arch)?;
    Ok(Some(HostUpdateTarget {
        game_dir,
        slot: assessment.slot,
        arch,
        action: assessment.action,
        conflict: assessment.conflict,
        source,
        channel,
        target_path: assessment.target_path,
    }))
}

/// Builds the tracked ReShade host-binary source recorded after a host download,
/// stamping the channel that produced it when known. Shared by every flow that
/// records a freshly downloaded or reused ReShade host artifact.
pub(crate) fn host_binary_source(
    url: String,
    etag: Option<String>,
    digest: String,
    last_modified: Option<String>,
    channel: Option<ReshadeChannel>,
) -> TrackedSource {
    let mut source = TrackedSource::new(TrackedSourceRole::HostBinary, url, etag, digest)
        .with_last_modified(last_modified);
    if let Some(channel) = channel {
        source = source.with_channel(channel.as_str());
    }
    source
}

/// A file an update flow (RenoDX add-on/host/DLSS-Fix update, ReShade channel
/// switch) overwrites in place with **no on-disk backup** — the artifact is a
/// rolling upstream snapshot or an official redistributable already
/// PE-sanity-checked on the way in, so nothing about the previous bytes is
/// worth preserving for manual recovery. `write_file_atomically` (temp+rename,
/// inside [`engine::replace_file`]) makes each individual write crash-safe on
/// its own; no separate engine sentinel is needed for this in-place path.
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
    replacements: &[Replacement],
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
            path: replacement.path.clone(),
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
        Err(errors::failed(format!(
            "failed to restore {failures} updated RenoDX file(s)"
        )))
    }
}

/// Same as [`restore_originals`], but only logs a failure rather than
/// returning one — used when the caller is already on an error path reporting
/// a different, primary failure.
pub(crate) fn restore_originals_best_effort(originals: &[OriginalFile]) {
    let failures = restore_originals_inner(originals);
    if failures > 0 {
        log::warn!("RenoDX update rollback failed to restore {failures} file(s)");
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
                "RenoDX update rollback: failed to restore `{}`: {error}",
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
    errors::failed(format!(
        "failed to save the update ({db_error}), and the on-disk rollback also failed \
         ({}); the game's files may not match its recorded state",
        restore_failures.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn read(path: &Path) -> Vec<u8> {
        std::fs::read(path).expect("file should exist")
    }

    #[test]
    fn apply_replacements_captures_originals_and_writes_new_bytes() {
        let dir = tempdir().expect("tempdir");
        let existing = dir.path().join("renodx-cp2077.addon64");
        std::fs::write(&existing, b"old-addon").expect("write");
        let fresh = dir.path().join("renodx-dlssfix.addon64");

        let originals = apply_replacements(&[
            Replacement {
                path: existing.clone(),
                bytes: b"new-addon".to_vec(),
                mtime: None,
            },
            Replacement {
                path: fresh.clone(),
                bytes: b"new-dlssfix".to_vec(),
                mtime: None,
            },
        ])
        .expect("apply");

        assert_eq!(read(&existing), b"new-addon");
        assert_eq!(read(&fresh), b"new-dlssfix");
        assert_eq!(originals[0].bytes.as_deref(), Some(&b"old-addon"[..]));
        assert_eq!(originals[1].bytes, None);
    }

    #[test]
    fn apply_replacements_rolls_back_everything_written_so_far_on_a_mid_loop_failure() {
        let dir = tempdir().expect("tempdir");
        let existing = dir.path().join("renodx-cp2077.addon64");
        std::fs::write(&existing, b"old-addon").expect("write");
        // A path with no parent directory at all can never be written to —
        // forces `engine::replace_file`'s second call to fail.
        let unwritable = PathBuf::from("");

        let error = apply_replacements(&[
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
        assert!(matches!(error, ServiceError::CommandFailed(_)));

        // The first replacement's write is rolled back to its pre-call bytes.
        assert_eq!(read(&existing), b"old-addon");
    }

    #[test]
    fn restore_originals_restores_bytes_or_deletes_when_none_existed() {
        let dir = tempdir().expect("tempdir");
        let existed_before = dir.path().join("dxgi.dll");
        let created_fresh = dir.path().join("renodx-cp2077.addon64");
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
        let db_error = errors::failed("db unavailable".to_owned());

        let error = persistence_failure_error(db_error, &[Ok(()), Ok(())]);

        match error {
            ServiceError::CommandFailed(message) => assert_eq!(message, "db unavailable"),
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }

    #[test]
    fn persistence_failure_error_combines_db_and_rollback_failures() {
        let db_error = errors::failed("db unavailable".to_owned());
        let rollback_error = errors::failed("disk full".to_owned());

        let error = persistence_failure_error(db_error, &[Ok(()), Err(rollback_error)]);

        match error {
            ServiceError::CommandFailed(message) => {
                assert!(message.contains("db unavailable"), "{message}");
                assert!(message.contains("disk full"), "{message}");
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }

    #[test]
    fn host_binary_source_sets_channel_when_provided() {
        let source = host_binary_source(
            "https://reshade.me/downloads/ReShade_Setup.exe".to_owned(),
            Some("etag".to_owned()),
            "digest".to_owned(),
            Some("Tue, 17 Jun 2026 09:00:00 GMT".to_owned()),
            Some(ReshadeChannel::Nightly),
        );

        assert_eq!(source.role(), TrackedSourceRole::HostBinary);
        assert_eq!(source.channel(), Some("nightly"));
        assert_eq!(source.digest(), "digest");
    }

    #[test]
    fn host_binary_source_omits_channel_when_not_provided() {
        let source = host_binary_source(
            "https://reshade.me/downloads/ReShade_Setup.exe".to_owned(),
            None,
            "digest".to_owned(),
            None,
            None,
        );

        assert_eq!(source.channel(), None);
    }
}
