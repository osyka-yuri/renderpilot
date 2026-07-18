use std::path::Path;

use renderpilot_domain::{InstalledAddon, LumaInstallState, TrackedSource};

use crate::ServiceError;
use crate::addons::reshade::fetch::sha256_hex;
use crate::addons::reshade::types::ReshadeChannel;
use crate::addons::reshade::update::host_binary_source;
use crate::addons::tracking;

/// Derives the `LumaInstallState` from the persisted record. `launch_args` is
/// re-resolved from the manifest by the caller (never persisted on the record
/// itself — see [`renderpilot_domain::LumaInstallState::Installed::launch_args`]).
pub(in crate::addons::luma) fn install_state_from_record(
    record: &InstalledAddon,
    launch_args: Vec<String>,
) -> LumaInstallState {
    let installed_at = record.installed_at().unwrap_or_else(|| {
        log::warn!(
            "Luma install record for `{}` is missing installed_at; emitting 0 for wire compatibility",
            record.game_id()
        );
        0
    });
    let updated_at = record.updated_at().unwrap_or_else(|| {
        log::warn!(
            "Luma install record for `{}` is missing updated_at; emitting 0 for wire compatibility",
            record.game_id()
        );
        0
    });
    LumaInstallState::Installed {
        version: record.addon_version().map(str::to_owned),
        addon_dated: tracking::effective_addon_dated(record),
        installed_at,
        updated_at,
        reshade_channel: record.reshade_channel().map(str::to_owned),
        launch_args,
    }
}

/// Reconstructs the only ReShade provenance Luma can safely infer from an
/// adopted empty host. Luma's host contract is always nightly, while the digest
/// is read from the exact DLL presently owned by the record; no release ZIP,
/// HTTP validator, or download history is invented.
pub(crate) fn advisory_nightly_host_source(
    host_path: &Path,
    nightly_url: String,
) -> Result<TrackedSource, ServiceError> {
    let bytes = std::fs::read(host_path).map_err(|error| {
        ServiceError::command_failed(format!(
            "failed to read adopted Luma ReShade host `{}`: {error}",
            host_path.display()
        ))
    })?;
    Ok(host_binary_source(
        nightly_url,
        None,
        sha256_hex(&bytes),
        None,
        Some(ReshadeChannel::Nightly),
    )
    .with_advisory())
}
