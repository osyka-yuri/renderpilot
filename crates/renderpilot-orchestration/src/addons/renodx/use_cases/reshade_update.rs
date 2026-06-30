/// Shared helpers for ReShade host update queries and commands.
use std::path::{Path, PathBuf};

use renderpilot_application::GameRepository;
use renderpilot_domain::{Architecture, GameId, InstalledAddon, TrackedSource, TrackedSourceRole};

use crate::{Context, ServiceError};

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

/// Resolves the target ReShade host and source for a proxy-host update.
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
