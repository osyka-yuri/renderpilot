//! Fetching the add-on and ReShade host for a RenoDX install, from upstream.
//!
//! Nothing is hashed against the manifest (add-ons are rolling snapshots); instead
//! the bytes are sanity-checked as a well-formed PE, the SHA-256 of the installed
//! add-on is recorded for update *detection*, and the response's cache validators
//! (ETag/Last-Modified) are captured. The add-on-enabled ReShade host is the
//! nightly.link CI zip, from which `ReShade{64,32}.dll` is extracted (the reshade.me
//! "stable" installer is an NSIS archive and is not supported — see [`super::source`]).

use std::io::{Cursor, Read};
use std::path::Path;

use renderpilot_domain::{Architecture, GameId};
use sha2::{Digest, Sha256};

use crate::net::{download_with_referer, download_with_validators, ProgressObserver};
use crate::ServiceError;

use super::errors;
use super::install::PreparedInstall;
use super::matcher::ResolvedInstall;
use super::reshade::{detect_reshade, ReshadeState};
use super::source;
use super::types::{ReshadeConfig, ReshadeIniTweaks};

/// An add-on DLL is small; cap well under that.
const MAX_ADDON_BYTES: u64 = 64 * 1024 * 1024;
/// The nightly ReShade zip is a few MB.
const MAX_RESHADE_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
/// GitHub artifact downloads (nightly.link) expect a GitHub referer.
const NIGHTLY_REFERER: &str = "https://github.com";

/// Fetches everything needed to install `resolved` into `game_dir`.
///
/// Always downloads the add-on; downloads a ReShade host only when none is already
/// present. Returns the [`PreparedInstall`] the engine lays down.
pub(super) async fn prepare_install(
    resolved: &ResolvedInstall,
    reshade_config: &ReshadeConfig,
    game_dir: &Path,
    game_id: GameId,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedInstall, ServiceError> {
    let label = format!("RenoDX add-on {}", resolved.slug);
    let (addon_bytes, validators) =
        download_with_validators(&resolved.addon_url, MAX_ADDON_BYTES, &label, progress).await?;
    let source = AddonSource {
        bytes: addon_bytes,
        url: resolved.addon_url.clone(),
        etag: validators.cache_validator(),
        last_modified: validators.last_modified,
    };
    build_prepared_install(
        resolved,
        reshade_config,
        game_dir,
        game_id,
        source,
        progress,
    )
    .await
}

/// Prepares an install from a **user-provided add-on file** instead of an upstream
/// download (for an external, Discord/Nexus-distributed game).
///
/// The bytes are PE-sanity-checked the same way a download is, the nightly ReShade
/// host is fetched only when none is present, and the record carries **no upstream
/// source** (empty URL, no validator) so the update system reports `Unknown` — a
/// file install has nothing to track upstream.
pub(super) async fn prepare_install_from_file(
    resolved: &ResolvedInstall,
    reshade_config: &ReshadeConfig,
    game_dir: &Path,
    game_id: GameId,
    addon_bytes: Vec<u8>,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedInstall, ServiceError> {
    let source = AddonSource {
        bytes: addon_bytes,
        url: String::new(),
        etag: None,
        last_modified: None,
    };
    build_prepared_install(
        resolved,
        reshade_config,
        game_dir,
        game_id,
        source,
        progress,
    )
    .await
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

/// A re-downloaded, PE-checked file (the RenoDX add-on, the DLSS-Fix companion, or
/// the extracted ReShade host DLL) plus the upstream identity recorded as a
/// [`TrackedSource`].
pub(super) struct Download {
    pub bytes: Vec<u8>,
    /// SHA-256 of the bytes — the durable change-detection digest.
    pub digest: String,
    /// ETag (or `Last-Modified` fallback) for the fast-path change pre-check.
    pub etag: Option<String>,
    /// Raw `Last-Modified` HTTP-date string, when the host sent one.
    pub last_modified: Option<String>,
}

/// Shared core of [`prepare_install`] and [`prepare_install_from_file`]: PE-checks
/// the add-on bytes, fetches the ReShade host when needed, and assembles the
/// [`PreparedInstall`].
async fn build_prepared_install(
    resolved: &ResolvedInstall,
    reshade_config: &ReshadeConfig,
    game_dir: &Path,
    game_id: GameId,
    source: AddonSource,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedInstall, ServiceError> {
    ensure_pe(&source.bytes, "RenoDX add-on")?;
    let source_digest = sha256_hex(&source.bytes);

    let reshade =
        fetch_reshade_host_if_needed(reshade_config, resolved.arch, game_dir, progress).await?;

    Ok(PreparedInstall {
        game_id,
        proxy_dll_name: resolved.proxy_dll_name.clone(),
        addon_file_name: addon_file_name(&resolved.slug, resolved.arch),
        addon_source_url: source.url,
        source_digest,
        source_etag: source.etag,
        source_last_modified: source.last_modified,
        addon_bytes: source.bytes,
        reshade_dll_bytes: reshade.bytes,
        // The nightly.link CI build has no stable version string to record.
        reshade_version: None,
        reshade_source_url: reshade.source_url,
        reshade_source_etag: reshade.etag,
        reshade_last_modified: reshade.last_modified,
        reshade_digest: reshade.digest,
        ini_tweaks: ReshadeIniTweaks::renodx_defaults(),
    })
}

/// On-disk add-on file name. A catalogue title uses its slug
/// (`renodx-<slug>.addon64`); a generic manual install (empty slug) falls back to a
/// stable `renodx-manual` name so the file is still well-formed and reversible.
fn addon_file_name(slug: &str, arch: Architecture) -> String {
    let base = if slug.is_empty() { "manual" } else { slug };
    format!("renodx-{base}.{}", arch.addon_extension())
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

fn needs_reshade_host(game_dir: &Path) -> bool {
    matches!(detect_reshade(game_dir), ReshadeState::Absent)
}

/// The ReShade host DLL bytes plus the upstream identity to track it for updates.
/// All fields are empty/None for [`FetchedReshade::none`], used when a host is
/// already present and we install none.
struct FetchedReshade {
    /// The extracted `ReShade*.dll` bytes (empty when no host is installed).
    pub bytes: Vec<u8>,
    /// The nightly zip URL the host came from (empty when none installed).
    pub source_url: String,
    /// The nightly zip's cache validator, for a cheap host update pre-check.
    pub etag: Option<String>,
    /// The nightly zip's `Last-Modified` HTTP-date string, when sent.
    pub last_modified: Option<String>,
    /// SHA-256 of the extracted DLL, the durable host change-detection digest.
    pub digest: String,
}

impl FetchedReshade {
    fn none() -> Self {
        Self {
            bytes: Vec::new(),
            source_url: String::new(),
            etag: None,
            last_modified: None,
            digest: String::new(),
        }
    }
}

/// Fetches the nightly ReShade host only when the game folder has none; otherwise
/// returns [`FetchedReshade::none`] (a foreign/existing host is reused untouched).
async fn fetch_reshade_host_if_needed(
    config: &ReshadeConfig,
    arch: Architecture,
    game_dir: &Path,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<FetchedReshade, ServiceError> {
    if needs_reshade_host(game_dir) {
        fetch_reshade_dll(config, arch, progress).await
    } else {
        Ok(FetchedReshade::none())
    }
}

/// Downloads the nightly ReShade zip, extracts the host DLL, and records the
/// upstream identity (URL + validator + DLL digest) for host update detection.
async fn fetch_reshade_dll(
    config: &ReshadeConfig,
    arch: Architecture,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<FetchedReshade, ServiceError> {
    let url = source::reshade_nightly_url(config, arch);
    let download = fetch_reshade_from_url(&url, arch, progress).await?;
    Ok(FetchedReshade {
        bytes: download.bytes,
        source_url: url,
        etag: download.etag,
        last_modified: download.last_modified,
        digest: download.digest,
    })
}

/// Re-downloads the ReShade host from a *recorded* zip URL (for a host update),
/// returning the extracted DLL bytes, the new cache validator, and the DLL digest.
/// `arch` selects which `ReShade*.dll` to extract.
pub(super) async fn fetch_reshade_from_url(
    url: &str,
    arch: Architecture,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<Download, ServiceError> {
    let (zip, validators) = download_with_referer(
        url,
        NIGHTLY_REFERER,
        MAX_RESHADE_ARCHIVE_BYTES,
        "ReShade nightly",
        progress,
    )
    .await?;
    let bytes = extract_reshade_dll(&zip, arch)?;
    let digest = sha256_hex(&bytes);
    Ok(Download {
        bytes,
        digest,
        etag: validators.cache_validator(),
        last_modified: validators.last_modified,
    })
}

/// Extracts `ReShade64.dll`/`ReShade32.dll` from the nightly zip, matching the
/// entry by base name (some artifacts nest it).
fn extract_reshade_dll(archive: &[u8], arch: Architecture) -> Result<Vec<u8>, ServiceError> {
    let target = match arch {
        Architecture::X64 => "ReShade64.dll",
        Architecture::X86 => "ReShade32.dll",
    };
    let mut zip = zip::ZipArchive::new(Cursor::new(archive))
        .map_err(|error| errors::failed(format!("ReShade archive is not a valid zip: {error}")))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| errors::failed(format!("failed to read ReShade archive: {error}")))?;
        let base = entry
            .name()
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(entry.name())
            .to_owned();
        if base.eq_ignore_ascii_case(target) {
            let mut buf = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
            entry
                .read_to_end(&mut buf)
                .map_err(|error| errors::failed(format!("failed to extract {target}: {error}")))?;
            ensure_pe(&buf, target)?;
            return Ok(buf);
        }
    }
    Err(errors::failed(format!(
        "{target} not found in the ReShade archive"
    )))
}

/// A RenoDX add-on and the ReShade DLL are both PE binaries; reject anything that
/// is not (a truncated download or an HTML error page).
fn ensure_pe(bytes: &[u8], what: &str) -> Result<(), ServiceError> {
    if bytes.len() < 64 || &bytes[..2] != b"MZ" {
        return Err(errors::failed(format!(
            "{what} download is not a valid PE binary ({} bytes)",
            bytes.len()
        )));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_pe_addon() {
        assert!(ensure_pe(b"<!doctype html>", "add-on").is_err());
        let mut pe = vec![b'M', b'Z'];
        pe.extend(std::iter::repeat_n(0u8, 100));
        assert!(ensure_pe(&pe, "add-on").is_ok());
    }
}
