//! Fetching the add-on and ReShade host for a RenoDX install, from upstream.
//!
//! Nothing is hashed against the manifest (add-ons are rolling snapshots); instead
//! the bytes are sanity-checked as a well-formed PE, the SHA-256 of the installed
//! add-on is recorded for update *detection*, and the response's cache validators
//! (ETag/Last-Modified) are captured. The add-on-enabled ReShade host can be the
//! manifest-current reshade.me stable add-on installer or the nightly.link CI zip;
//! in both cases the recorded host digest is the extracted DLL's digest.

use renderpilot_domain::{Architecture, GameId};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};

use crate::ServiceError;
use crate::net::{
    ProgressObserver, download_with_referer, download_with_validators,
    download_with_validators_and_final_url,
};

use super::errors;
use super::install::PreparedInstall;
use super::matcher::ResolvedInstall;
use super::policy::HostKind;
use super::source;
use super::types::{ReshadeChannel, ReshadeConfig, ReshadeIniTweaks};

/// An add-on DLL is small; cap well under that.
const MAX_ADDON_BYTES: u64 = 64 * 1024 * 1024;
/// ReShade source archives are a few MB.
const MAX_RESHADE_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
/// The extracted ReShade DLL should stay comfortably below this ceiling.
const MAX_RESHADE_DLL_BYTES: u64 = 64 * 1024 * 1024;
/// GitHub artifact downloads (nightly.link) expect a GitHub referer.
const NIGHTLY_REFERER: &str = "https://github.com";

/// Fetches everything needed to install `resolved` into `game_dir`.
///
/// Always downloads the add-on; downloads a ReShade host only when the shared
/// host policy decided the install must write one. Returns the
/// [`PreparedInstall`] the engine lays down.
pub(super) async fn prepare_install(
    resolved: &ResolvedInstall,
    reshade_config: &ReshadeConfig,
    game_id: GameId,
    channel: ReshadeChannel,
    writes_host: bool,
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
        game_id,
        source,
        channel,
        writes_host,
        progress,
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
    reshade_config: &ReshadeConfig,
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

/// A re-downloaded, PE-checked file (the RenoDX add-on, the DLSS-Fix companion, or
/// the extracted ReShade host DLL) plus the upstream identity recorded as a
/// [`TrackedSource`].
pub(crate) struct Download {
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
        ini_tweaks: ReshadeIniTweaks::renodx_defaults(),
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
    config: &ReshadeConfig,
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
    config: &ReshadeConfig,
    arch: Architecture,
    channel: ReshadeChannel,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<FetchedReshade, ServiceError> {
    let source = source::require_reshade_source(config, channel, arch)?;
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

/// Re-downloads the ReShade host from a concrete channel source,
/// returning the extracted DLL bytes, the new cache validator, and the DLL digest.
/// `arch` selects which `ReShade*.dll` to extract.
pub(super) async fn fetch_reshade_from_source(
    source: &source::ReshadeSource,
    arch: Architecture,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<Download, ServiceError> {
    let (zip, validators) = match source.channel {
        ReshadeChannel::Stable => {
            let (zip, validators, final_url) = download_with_validators_and_final_url(
                &source.url,
                MAX_RESHADE_ARCHIVE_BYTES,
                "ReShade stable",
                progress,
            )
            .await?;
            ensure_stable_final_url(&final_url)?;
            (zip, validators)
        }
        ReshadeChannel::Nightly => {
            download_with_referer(
                &source.url,
                NIGHTLY_REFERER,
                MAX_RESHADE_ARCHIVE_BYTES,
                "ReShade nightly",
                progress,
            )
            .await?
        }
    };
    let bytes = extract_reshade_dll(&zip, arch)?;
    ensure_pe_arch(&bytes, arch, "ReShade host")?;
    let digest = sha256_hex(&bytes);
    Ok(Download {
        bytes,
        digest,
        etag: validators.cache_validator(),
        last_modified: validators.last_modified,
    })
}

fn ensure_stable_final_url(url: &reqwest::Url) -> Result<(), ServiceError> {
    if url.scheme() != "https" || url.host_str() != Some("reshade.me") {
        return Err(errors::failed(format!(
            "ReShade stable download redirected to an untrusted URL: {url}"
        )));
    }
    Ok(())
}

/// Extracts `ReShade64.dll`/`ReShade32.dll` from a zip-compatible ReShade
/// archive/SFX, matching the entry by base name (some artifacts nest it).
fn extract_reshade_dll(archive: &[u8], arch: Architecture) -> Result<Vec<u8>, ServiceError> {
    let target = match arch {
        Architecture::X64 => "ReShade64.dll",
        Architecture::X86 => "ReShade32.dll",
    };
    let mut zip = zip::ZipArchive::new(Cursor::new(archive))
        .map_err(|error| errors::failed(format!("ReShade archive is not a valid zip: {error}")))?;
    let mut match_bytes: Option<Vec<u8>> = None;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| errors::failed(format!("failed to read ReShade archive: {error}")))?;
        let Some(enclosed) = entry.enclosed_name() else {
            return Err(errors::failed(format!(
                "ReShade archive contains an unsafe path `{}`",
                entry.name()
            )));
        };
        let base = enclosed
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        if base.eq_ignore_ascii_case(target) {
            if match_bytes.is_some() {
                return Err(errors::failed(format!(
                    "ReShade archive contains multiple `{target}` candidates"
                )));
            }
            if entry.compressed_size() > MAX_RESHADE_DLL_BYTES
                || entry.size() > MAX_RESHADE_DLL_BYTES
            {
                return Err(errors::failed(format!(
                    "{target} in the ReShade archive is too large"
                )));
            }
            let mut buf = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
            let limit = MAX_RESHADE_DLL_BYTES + 1;
            entry
                .by_ref()
                .take(limit)
                .read_to_end(&mut buf)
                .map_err(|error| errors::failed(format!("failed to extract {target}: {error}")))?;
            if buf.len() as u64 > MAX_RESHADE_DLL_BYTES {
                return Err(errors::failed(format!(
                    "{target} in the ReShade archive exceeds the size limit"
                )));
            }
            ensure_pe(&buf, target)?;
            match_bytes = Some(buf);
        }
    }
    match_bytes.ok_or_else(|| errors::failed(format!("{target} not found in the ReShade archive")))
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

fn ensure_pe_arch(bytes: &[u8], expected: Architecture, what: &str) -> Result<(), ServiceError> {
    ensure_pe(bytes, what)?;
    let actual =
        renderpilot_detection::read_pe_architecture_from_bytes(bytes).ok_or_else(|| {
            errors::failed(format!(
                "{what} download has an unsupported PE machine type"
            ))
        })?;
    if actual != expected {
        return Err(errors::failed(format!(
            "{what} architecture mismatch: expected {expected:?}, got {actual:?}"
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
    use std::io::Write;

    use super::*;
    use crate::addons::renodx::test_support::{
        MACHINE_AMD64, MACHINE_I386, PE32_MAGIC, PE32_PLUS_MAGIC, build_pe_with_exports,
    };

    #[test]
    fn rejects_non_pe_addon() {
        assert!(ensure_pe(b"<!doctype html>", "add-on").is_err());
        let mut pe = vec![b'M', b'Z'];
        pe.extend(std::iter::repeat_n(0u8, 100));
        assert!(ensure_pe(&pe, "add-on").is_ok());
    }

    #[test]
    fn stable_final_url_must_stay_on_official_https_host() {
        let official =
            reqwest::Url::parse("https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe")
                .expect("url");
        let wrong_host =
            reqwest::Url::parse("https://example.com/downloads/ReShade_Setup_6.7.3_Addon.exe")
                .expect("url");
        let wrong_scheme =
            reqwest::Url::parse("http://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe")
                .expect("url");

        assert!(ensure_stable_final_url(&official).is_ok());
        assert!(ensure_stable_final_url(&wrong_host).is_err());
        assert!(ensure_stable_final_url(&wrong_scheme).is_err());
    }

    #[test]
    fn extracts_reshade_from_zip_compatible_stable_sfx() {
        let dll = build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]);
        let mut archive = b"MZ fake installer prefix".to_vec();
        archive.extend(zip_with_entries(&[(
            "nested/ReShade64.dll",
            dll.as_slice(),
        )]));

        let bytes = extract_reshade_dll(&archive, Architecture::X64).expect("extract");

        assert_eq!(bytes, dll);
    }

    #[test]
    fn rejects_unsafe_reshade_archive_paths() {
        let dll = build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]);
        let archive = zip_with_entries(&[("../ReShade64.dll", dll.as_slice())]);

        assert!(extract_reshade_dll(&archive, Architecture::X64).is_err());
    }

    #[test]
    fn rejects_duplicate_reshade_candidates() {
        let dll = build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]);
        let archive = zip_with_entries(&[
            ("a/ReShade64.dll", dll.as_slice()),
            ("b/ReShade64.dll", dll.as_slice()),
        ]);

        assert!(extract_reshade_dll(&archive, Architecture::X64).is_err());
    }

    #[test]
    fn rejects_reshade_architecture_mismatch() {
        let dll = build_pe_with_exports(MACHINE_I386, PE32_MAGIC, &[]);
        let archive = zip_with_entries(&[("ReShade64.dll", dll.as_slice())]);
        let extracted = extract_reshade_dll(&archive, Architecture::X64).expect("extract");

        assert!(ensure_pe_arch(&extracted, Architecture::X64, "ReShade host").is_err());
    }

    fn zip_with_entries(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .expect("start file");
            zip.write_all(bytes).expect("write entry");
        }
        zip.finish().expect("finish zip").into_inner()
    }
}
