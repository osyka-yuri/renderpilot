//! Fetching the add-on and ReShade host for a RenoDX install, from upstream.
//!
//! Nothing is hashed against the manifest (add-ons are rolling snapshots); instead
//! the bytes are sanity-checked as a well-formed PE, the SHA-256 of the installed
//! add-on is recorded for update *detection*, and the response's cache validators
//! (ETag/Last-Modified) are captured. The ReShade host download/extraction is
//! shared ([`crate::addons::reshade::fetch`]); this module orchestrates the RenoDX
//! add-on download and bundles the [`PreparedInstall`] the engine lays down.

use renderpilot_domain::{Architecture, GameId};

use crate::ServiceError;
use crate::net::{ProgressObserver, download_with_validators};

use super::install::PreparedInstall;
use super::matcher::ResolvedInstall;
use super::source;
use super::types::renodx_ini_defaults;
use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};

use crate::addons::progress::sequential_stage_observer;
use crate::addons::reshade::fetch::{Download, ensure_pe, fetch_reshade_from_source, sha256_hex};
use crate::addons::reshade::source::require_reshade_source;

/// An add-on DLL is small; cap well under that.
const MAX_ADDON_BYTES: u64 = 64 * 1024 * 1024;

/// Fetches everything needed to install `resolved` into `game_dir`.
///
/// Always downloads the add-on; downloads a ReShade host only when the shared
/// host policy decided the install must write one. Returns the
/// [`PreparedInstall`] the engine lays down.
pub(super) async fn prepare_install(
    resolved: &ResolvedInstall,
    reshade_config: &ReshadeSourceCatalog,
    game_id: GameId,
    channel: ReshadeChannel,
    writes_host: bool,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedInstall, ServiceError> {
    let downloads_host = matches!(resolved.host_kind, HostKind::Proxy) && writes_host;
    let stage_count = 1 + u64::from(downloads_host);
    let addon_progress_fn = sequential_stage_observer(progress, 0, stage_count);
    let addon_progress = addon_progress_fn
        .as_ref()
        .map(|observer| observer as &ProgressObserver<'_>);
    let label = format!("RenoDX add-on {}", resolved.slug);
    let (addon_bytes, validators) =
        download_with_validators(&resolved.addon_url, MAX_ADDON_BYTES, &label, addon_progress)
            .await?;
    let source = AddonSource {
        bytes: addon_bytes,
        url: resolved.addon_url.clone(),
        etag: validators.cache_validator(),
        last_modified: validators.last_modified,
    };
    let host_progress_fn = if downloads_host {
        sequential_stage_observer(progress, 1, stage_count)
    } else {
        None
    };
    let host_progress = host_progress_fn
        .as_ref()
        .map(|observer| observer as &ProgressObserver<'_>);
    build_prepared_install(
        resolved,
        reshade_config,
        game_id,
        source,
        channel,
        writes_host,
        host_progress,
    )
    .await
}

/// Prepares an install from a **user-provided add-on file** instead of an upstream
/// download (for an external, Discord/Nexus-distributed game).
///
/// The bytes are PE-sanity-checked the same way a download is, the ReShade
/// host is fetched only when the shared host policy decided the install must
/// write one, and the record carries **no upstream source** (empty URL, no
/// validator) so the update system reports `Unknown` — a file install has nothing
/// to track upstream.
pub(super) async fn prepare_install_from_file(
    resolved: &ResolvedInstall,
    reshade_config: &ReshadeSourceCatalog,
    game_id: GameId,
    addon: LocalAddonSource,
    channel: ReshadeChannel,
    writes_host: bool,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedInstall, ServiceError> {
    let source = AddonSource {
        bytes: addon.bytes,
        url: String::new(),
        etag: None,
        last_modified: addon.last_modified,
    };
    build_prepared_install(
        resolved,
        reshade_config,
        game_id,
        source,
        channel,
        writes_host,
        progress,
    )
    .await
}

pub(super) struct LocalAddonSource {
    pub bytes: Vec<u8>,
    pub last_modified: Option<String>,
}

/// The add-on bytes plus their upstream identity (URL + cache validators). The URL
/// is empty and the validators `None` for a user-provided file install.
struct AddonSource {
    bytes: Vec<u8>,
    url: String,
    etag: Option<String>,
    /// Raw `Last-Modified` HTTP-date string, when the host sent one.
    last_modified: Option<String>,
}

/// Shared core of [`prepare_install`] and [`prepare_install_from_file`]: PE-checks
/// the add-on bytes, fetches the ReShade host when needed, and assembles the
/// [`PreparedInstall`].
async fn build_prepared_install(
    resolved: &ResolvedInstall,
    reshade_config: &ReshadeSourceCatalog,
    game_id: GameId,
    source: AddonSource,
    channel: ReshadeChannel,
    writes_host: bool,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedInstall, ServiceError> {
    ensure_pe(&source.bytes, "RenoDX add-on")?;
    let source_digest = sha256_hex(&source.bytes);

    let reshade = match resolved.host_kind {
        HostKind::Vulkan => FetchedReshade::none(),
        HostKind::Proxy => {
            fetch_reshade_host_if_needed(
                reshade_config,
                resolved.arch,
                channel,
                writes_host,
                progress,
            )
            .await?
        }
    };

    Ok(PreparedInstall {
        game_id,
        host_kind: resolved.host_kind,
        proxy_dll_name: resolved.proxy_dll_name.clone(),
        addon_file_name: source::addon_file_name(&resolved.slug, resolved.arch),
        addon_source_url: source.url,
        source_digest,
        source_etag: source.etag,
        source_last_modified: source.last_modified,
        addon_bytes: source.bytes,
        reshade_dll_bytes: reshade.bytes,
        reshade_source_url: reshade.source_url,
        reshade_source_etag: reshade.etag,
        reshade_last_modified: reshade.last_modified,
        reshade_digest: reshade.digest,
        reshade_channel: reshade.channel,
        ini_tweaks: renodx_ini_defaults(),
    })
}

/// Re-downloads just the add-on (for an update), returning bytes + new validators.
pub(super) async fn fetch_addon(
    addon_url: &str,
    slug: &str,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<Download, ServiceError> {
    let label = format!("RenoDX add-on {slug}");
    let (bytes, validators) =
        download_with_validators(addon_url, MAX_ADDON_BYTES, &label, progress).await?;
    ensure_pe(&bytes, "RenoDX add-on")?;
    let digest = sha256_hex(&bytes);
    Ok(Download {
        bytes,
        digest,
        etag: validators.cache_validator(),
        last_modified: validators.last_modified,
    })
}

/// Fetches the DLSS-Fix companion add-on for `arch`, returning the PE-checked
/// bytes, digest, and validators. A thin wrapper over [`fetch_addon`] that derives
/// the arch-specific URL from the `dlssfix` slug.
pub(super) async fn fetch_dlss_fix(
    arch: Architecture,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<Download, ServiceError> {
    fetch_addon(&source::dlss_fix_url(arch), "DLSS-Fix", progress).await
}

/// The ReShade host DLL bytes plus the upstream identity to track it for updates.
/// All fields are empty/None for [`FetchedReshade::none`], used when a host is
/// already present and we install none.
struct FetchedReshade {
    /// The extracted `ReShade*.dll` bytes (empty when no host is installed).
    pub bytes: Vec<u8>,
    /// The source archive URL the host came from (empty when none installed).
    pub source_url: String,
    /// The source archive's cache validator, for a cheap host update pre-check.
    pub etag: Option<String>,
    /// The source archive's `Last-Modified` HTTP-date string, when sent.
    pub last_modified: Option<String>,
    /// SHA-256 of the extracted DLL, the durable host change-detection digest.
    pub digest: String,
    /// Effective channel for this host artifact.
    pub channel: Option<ReshadeChannel>,
}

impl FetchedReshade {
    fn none() -> Self {
        Self {
            bytes: Vec::new(),
            source_url: String::new(),
            etag: None,
            last_modified: None,
            digest: String::new(),
            channel: None,
        }
    }
}

/// Fetches the requested-channel ReShade host only when policy says the active
/// host slot needs one; otherwise returns [`FetchedReshade::none`].
async fn fetch_reshade_host_if_needed(
    config: &ReshadeSourceCatalog,
    arch: Architecture,
    channel: ReshadeChannel,
    writes_host: bool,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<FetchedReshade, ServiceError> {
    if writes_host {
        fetch_reshade_dll(config, arch, channel, progress).await
    } else {
        Ok(FetchedReshade::none())
    }
}

/// Downloads the channel ReShade archive, extracts the host DLL, and records the
/// upstream identity (URL + validator + DLL digest) for host update detection.
async fn fetch_reshade_dll(
    config: &ReshadeSourceCatalog,
    arch: Architecture,
    channel: ReshadeChannel,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<FetchedReshade, ServiceError> {
    let source = require_reshade_source(config, channel, arch)?;
    let download = fetch_reshade_from_source(&source, arch, progress).await?;
    Ok(FetchedReshade {
        bytes: download.bytes,
        source_url: source.url,
        etag: download.etag,
        last_modified: download.last_modified,
        digest: download.digest,
        channel: Some(channel),
    })
}
