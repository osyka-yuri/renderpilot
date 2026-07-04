//! Downloading and extracting the add-on-enabled ReShade host DLL.
//!
//! The host can be the manifest-current reshade.me stable add-on installer or the
//! nightly.link CI zip; in both cases the recorded host digest is the extracted
//! DLL's digest, and the bytes are PE-sanity-checked (and architecture-checked)
//! before use.

use std::io::{Cursor, Read};

use renderpilot_domain::Architecture;
use sha2::{Digest, Sha256};

use super::super::errors::failed;
use super::source::ReshadeSource;
use super::types::ReshadeChannel;
use crate::ServiceError;
use crate::net::{ProgressObserver, download_with_referer, download_with_validators_and_final_url};

/// ReShade source archives are a few MB.
const MAX_RESHADE_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
/// The extracted ReShade DLL should stay comfortably below this ceiling.
const MAX_RESHADE_DLL_BYTES: u64 = 64 * 1024 * 1024;
/// GitHub artifact downloads (nightly.link) expect a GitHub referer.
const NIGHTLY_REFERER: &str = "https://github.com";

/// A re-downloaded, PE-checked file (an add-on payload or the extracted ReShade
/// host DLL) plus the upstream identity recorded as a `TrackedSource`.
pub(crate) struct Download {
    pub bytes: Vec<u8>,
    /// SHA-256 of the bytes — the durable change-detection digest.
    pub digest: String,
    /// ETag (or `Last-Modified` fallback) for the fast-path change pre-check.
    pub etag: Option<String>,
    /// Raw `Last-Modified` HTTP-date string, when the host sent one.
    pub last_modified: Option<String>,
}

/// Re-downloads the ReShade host from a concrete channel source, returning the
/// extracted DLL bytes, the new cache validator, and the DLL digest. `arch`
/// selects which `ReShade*.dll` to extract.
pub(crate) async fn fetch_reshade_from_source(
    source: &ReshadeSource,
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
        return Err(failed(format!(
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
        .map_err(|error| failed(format!("ReShade archive is not a valid zip: {error}")))?;
    let mut match_bytes: Option<Vec<u8>> = None;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| failed(format!("failed to read ReShade archive: {error}")))?;
        let Some(enclosed) = entry.enclosed_name() else {
            return Err(failed(format!(
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
                return Err(failed(format!(
                    "ReShade archive contains multiple `{target}` candidates"
                )));
            }
            if entry.compressed_size() > MAX_RESHADE_DLL_BYTES
                || entry.size() > MAX_RESHADE_DLL_BYTES
            {
                return Err(failed(format!(
                    "{target} in the ReShade archive is too large"
                )));
            }
            let mut buf = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
            let limit = MAX_RESHADE_DLL_BYTES + 1;
            entry
                .by_ref()
                .take(limit)
                .read_to_end(&mut buf)
                .map_err(|error| failed(format!("failed to extract {target}: {error}")))?;
            if buf.len() as u64 > MAX_RESHADE_DLL_BYTES {
                return Err(failed(format!(
                    "{target} in the ReShade archive exceeds the size limit"
                )));
            }
            ensure_pe(&buf, target)?;
            match_bytes = Some(buf);
        }
    }
    match_bytes.ok_or_else(|| failed(format!("{target} not found in the ReShade archive")))
}

/// A PE binary is required; reject anything that is not (a truncated download or
/// an HTML error page).
pub(crate) fn ensure_pe(bytes: &[u8], what: &str) -> Result<(), ServiceError> {
    if bytes.len() < 64 || &bytes[..2] != b"MZ" {
        return Err(failed(format!(
            "{what} download is not a valid PE binary ({} bytes)",
            bytes.len()
        )));
    }
    Ok(())
}

pub(crate) fn ensure_pe_arch(
    bytes: &[u8],
    expected: Architecture,
    what: &str,
) -> Result<(), ServiceError> {
    ensure_pe(bytes, what)?;
    let actual =
        renderpilot_detection::read_pe_architecture_from_bytes(bytes).ok_or_else(|| {
            failed(format!(
                "{what} download has an unsupported PE machine type"
            ))
        })?;
    if actual != expected {
        return Err(failed(format!(
            "{what} architecture mismatch: expected {expected:?}, got {actual:?}"
        )));
    }
    Ok(())
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::test_support::{
        MACHINE_AMD64, MACHINE_I386, PE32_MAGIC, PE32_PLUS_MAGIC, build_pe_with_exports,
        zip_with_entries,
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
}
