//! Allow-listed ZIP extraction for a Luma release asset (no network).

use std::collections::HashMap;
use std::io::{Cursor, Read};

use renderpilot_detection::NVNGX_DLSS_FILE_NAME;
use renderpilot_domain::Architecture;

use crate::ServiceError;
use crate::addons::reshade::fetch::{ensure_pe, ensure_pe_arch};

use super::super::errors;
use super::types::LumaPayloadFile;

/// A Luma release ZIP is small (a `.addon`, an optional `nvngx_dlss.dll`, and a
/// loose shader tree); cap well under that.
pub(super) const MAX_ZIP_BYTES: u64 = 128 * 1024 * 1024;
/// Upper bound on the sum of every extracted file's real (decompressed) size --
/// guards against a decompression bomb regardless of what the archive's central
/// directory claims.
const MAX_TOTAL_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
/// Upper bound on any single extracted file's real (decompressed) size.
const MAX_ENTRY_UNCOMPRESSED_BYTES: u64 = 96 * 1024 * 1024;
/// Upper bound on the number of entries a Luma archive may contain.
const MAX_ZIP_ENTRIES: usize = 2048;

/// Root-level bundled ReShade host RenderPilot never extracts -- the shared host
/// subsystem installs its own, channel-selected build instead.
const BUNDLED_DXGI: &str = "dxgi.dll";
/// Root-level bundled ReShade config RenderPilot never extracts (not currently
/// shipped by Luma, per the verified ZIP layout, but skipped defensively).
const BUNDLED_RESHADE_INI: &str = "reshade.ini";
/// The shader-tree folder every Luma asset ships, extracted verbatim.
const SHADER_TREE_ROOT: &str = "luma";

/// How a zip entry's normalized relative path is kept. `classify_entry` returns
/// `None` -- uniformly, with no further variant needed -- for everything that is
/// skipped, whether recognized-but-excluded (the bundled host / config) or
/// genuinely unrecognized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryClass {
    /// The single root `.addon` file.
    RootAddon,
    /// The optional root `nvngx_dlss.dll`.
    RootDlss,
    /// Anything under `Luma/`, extracted verbatim.
    Tree,
}

fn classify_entry(relative: &str) -> Option<EntryClass> {
    if let Some((first, rest)) = relative.split_once('/') {
        return (first.eq_ignore_ascii_case(SHADER_TREE_ROOT) && !rest.is_empty())
            .then_some(EntryClass::Tree);
    }
    let lower = relative.to_ascii_lowercase();
    if lower == BUNDLED_DXGI || lower == BUNDLED_RESHADE_INI {
        return None;
    }
    if lower == NVNGX_DLSS_FILE_NAME {
        return Some(EntryClass::RootDlss);
    }
    if lower.ends_with(".addon") {
        return Some(EntryClass::RootAddon);
    }
    None
}

/// Extracts the allow-listed payload files from a Luma release ZIP, returning
/// them plus the main add-on's relative path.
pub(super) fn extract_luma_payload(
    zip_bytes: &[u8],
    expected_addon_file: &str,
    arch: Architecture,
) -> Result<(Vec<LumaPayloadFile>, String), ServiceError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|error| errors::failed(format!("Luma archive is not a valid zip: {error}")))?;
    if zip.len() > MAX_ZIP_ENTRIES {
        return Err(errors::failed(format!(
            "Luma archive has too many entries (maximum {MAX_ZIP_ENTRIES})"
        )));
    }

    let mut files: Vec<LumaPayloadFile> = Vec::new();
    // Lowercased relative path -> index into `files`, for case-collision detection.
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut total_uncompressed: u64 = 0;
    let mut main_addon_rel: Option<String> = None;

    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| {
            errors::failed(format!("failed to read Luma archive entry: {error}"))
        })?;
        if entry.is_dir() {
            continue;
        }
        // `enclosed_name()` is the zip-slip safety gate (rejects `..`, absolute
        // paths, and other unsafe forms); the raw `/`-separated name is used for
        // the relative-path string once that gate has passed. A second,
        // independent validation runs at install time via
        // `engine::ensure_safe_relative_path` (see `FileOp::CreateNested`).
        if entry.enclosed_name().is_none() {
            return Err(errors::failed(format!(
                "Luma archive contains an unsafe path `{}`",
                entry.name()
            )));
        }
        let relative = entry.name().replace('\\', "/");
        let Some(classification) = classify_entry(&relative) else {
            log::debug!("Luma fetch: skipping entry `{relative}`");
            continue;
        };

        let capacity = usize::try_from(entry.size().min(MAX_ENTRY_UNCOMPRESSED_BYTES)).unwrap_or(0);
        let mut buf = Vec::with_capacity(capacity);
        let limit = MAX_ENTRY_UNCOMPRESSED_BYTES + 1;
        entry
            .by_ref()
            .take(limit)
            .read_to_end(&mut buf)
            .map_err(|error| errors::failed(format!("failed to extract `{relative}`: {error}")))?;
        if buf.len() as u64 > MAX_ENTRY_UNCOMPRESSED_BYTES {
            return Err(errors::failed(format!(
                "`{relative}` in the Luma archive exceeds the per-file size limit"
            )));
        }
        total_uncompressed = total_uncompressed.saturating_add(buf.len() as u64);
        if total_uncompressed > MAX_TOTAL_UNCOMPRESSED_BYTES {
            return Err(errors::failed(
                "Luma archive exceeds the total uncompressed size limit".to_owned(),
            ));
        }

        match classification {
            EntryClass::RootAddon => {
                if !relative.eq_ignore_ascii_case(expected_addon_file) {
                    return Err(errors::failed(format!(
                        "Luma archive root add-on `{relative}` does not match manifest `{expected_addon_file}`"
                    )));
                }
                ensure_pe(&buf, "Luma add-on")?;
                ensure_pe_arch(&buf, arch, "Luma add-on")?;
                if main_addon_rel.is_some() {
                    return Err(errors::failed(
                        "Luma archive contains more than one root `.addon` file".to_owned(),
                    ));
                }
                main_addon_rel = Some(relative.clone());
            }
            EntryClass::RootDlss => {
                // Same arch gate as the main `.addon` / ReShade host: a wrong-arch
                // DLSS would still extract and shadow a game-owned DLL (with backup).
                ensure_pe_arch(&buf, arch, NVNGX_DLSS_FILE_NAME)?;
                renderpilot_detection::DlssBinaryInfo::from_bytes(&buf).map_err(|error| {
                    errors::failed(format!(
                        "bundled {NVNGX_DLSS_FILE_NAME} failed NVIDIA DLSS identity/version validation: {error}"
                    ))
                })?;
            }
            EntryClass::Tree => {}
        }

        let key = relative.to_ascii_lowercase();
        if let Some(&existing_index) = seen.get(&key) {
            if files[existing_index].bytes == buf {
                log::debug!(
                    "Luma fetch: duplicate case-collision entry `{relative}` (identical bytes, skipped)"
                );
                continue;
            }
            return Err(errors::failed(format!(
                "Luma archive contains conflicting entries for `{relative}`"
            )));
        }
        seen.insert(key, files.len());
        files.push(LumaPayloadFile {
            relative_path: relative,
            bytes: buf,
        });
    }

    let main_addon_rel = main_addon_rel
        .ok_or_else(|| errors::failed("Luma archive contains no root `.addon` file".to_owned()))?;
    Ok((files, main_addon_rel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::luma::test_support::{
        MACHINE_AMD64, MACHINE_I386, PE32_MAGIC, PE32_PLUS_MAGIC, build_nvidia_dlss_pe,
        build_pe_with_exports, zip_with_entries,
    };

    fn addon_bytes() -> Vec<u8> {
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[])
    }

    #[test]
    fn extracts_manifest_identified_root_addon_and_tree_files_and_skips_bundled_host() {
        let addon = addon_bytes();
        let zip = zip_with_entries(&[
            ("Luma-Dishonored 2.addon", addon.as_slice()),
            ("dxgi.dll", b"bundled-reshade-shim"),
            ("Luma/Global/Copy_PS.hlsl", b"technique {}"),
            ("Luma/Includes/Common.hlsl", b"// common"),
            ("Luma/Dishonored 2/Fog_PS.hlsl", b"// fog"),
        ]);

        let (files, main) =
            extract_luma_payload(&zip, "Luma-Dishonored 2.addon", Architecture::X64)
                .expect("extract");

        assert_eq!(main, "Luma-Dishonored 2.addon");
        let paths: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(paths.contains(&"Luma-Dishonored 2.addon"));
        assert!(paths.contains(&"Luma/Global/Copy_PS.hlsl"));
        assert!(paths.contains(&"Luma/Includes/Common.hlsl"));
        assert!(paths.contains(&"Luma/Dishonored 2/Fog_PS.hlsl"));
        assert!(!paths.iter().any(|p| p.eq_ignore_ascii_case("dxgi.dll")));
    }

    #[test]
    fn extracts_optional_nvngx_dlss_at_root() {
        let addon = addon_bytes();
        let dlss = build_nvidia_dlss_pe([3, 7, 0, 0]);
        let zip = zip_with_entries(&[
            ("Luma-Game.addon", addon.as_slice()),
            ("nvngx_dlss.dll", dlss.as_slice()),
            ("Luma/Global/Copy_PS.hlsl", b"technique {}"),
        ]);

        let (files, _) =
            extract_luma_payload(&zip, "Luma-Game.addon", Architecture::X64).expect("extract");

        assert!(files.iter().any(|f| f.relative_path == "nvngx_dlss.dll"));
    }

    #[test]
    fn rejects_wrong_architecture_nvngx_dlss() {
        let addon = addon_bytes();
        let dlss_x86 = build_pe_with_exports(MACHINE_I386, PE32_MAGIC, &[]);
        let zip = zip_with_entries(&[
            ("Luma-Game.addon", addon.as_slice()),
            ("nvngx_dlss.dll", dlss_x86.as_slice()),
            ("Luma/Global/Copy_PS.hlsl", b"technique {}"),
        ]);

        let err = extract_luma_payload(&zip, "Luma-Game.addon", Architecture::X64)
            .expect_err("x86 DLSS must fail under X64 extract");
        let message = err.to_string();
        assert!(
            message.contains("architecture") || message.contains("nvngx_dlss"),
            "unexpected error: {message}"
        );
    }

    #[test]
    fn skips_a_bundled_reshade_ini_defensively() {
        let addon = addon_bytes();
        let zip = zip_with_entries(&[
            ("Luma-Game.addon", addon.as_slice()),
            ("ReShade.ini", b"[GENERAL]\r\n"),
            ("Luma/Global/Copy_PS.hlsl", b"technique {}"),
        ]);

        let (files, _) =
            extract_luma_payload(&zip, "Luma-Game.addon", Architecture::X64).expect("extract");

        assert!(
            !files
                .iter()
                .any(|f| f.relative_path.eq_ignore_ascii_case("reshade.ini"))
        );
    }

    #[test]
    fn rejects_a_zip_with_no_root_addon() {
        let zip = zip_with_entries(&[("Luma/Global/Copy_PS.hlsl", b"technique {}")]);
        assert!(extract_luma_payload(&zip, "Luma-Game.addon", Architecture::X64).is_err());
    }

    #[test]
    fn rejects_a_zip_with_more_than_one_root_addon() {
        let addon = addon_bytes();
        let zip = zip_with_entries(&[
            ("Luma-Game.addon", addon.as_slice()),
            ("Luma-Other.addon", addon.as_slice()),
        ]);
        assert!(extract_luma_payload(&zip, "Luma-Game.addon", Architecture::X64).is_err());
    }

    #[test]
    fn rejects_unsafe_archive_paths() {
        let addon = addon_bytes();
        let zip = zip_with_entries(&[
            ("Luma-Game.addon", addon.as_slice()),
            ("../escape.hlsl", b"evil"),
        ]);
        assert!(extract_luma_payload(&zip, "Luma-Game.addon", Architecture::X64).is_err());
    }

    #[test]
    fn rejects_main_addon_architecture_mismatch() {
        // A 32-bit add-on in a 64-bit fetch must be rejected.
        let addon_x86 = build_pe_with_exports(0x014c, 0x10b, &[]);
        let zip = zip_with_entries(&[("Luma-Game.addon", addon_x86.as_slice())]);
        assert!(extract_luma_payload(&zip, "Luma-Game.addon", Architecture::X64).is_err());
    }

    #[test]
    fn case_identical_duplicate_entries_are_tolerated() {
        let addon = addon_bytes();
        let zip = zip_with_entries(&[
            ("Luma-Game.addon", addon.as_slice()),
            ("Luma/Global/Copy_PS.hlsl", b"technique {}"),
            ("luma/global/copy_ps.hlsl", b"technique {}"),
        ]);
        let (files, _) =
            extract_luma_payload(&zip, "Luma-Game.addon", Architecture::X64).expect("extract");
        assert_eq!(
            files
                .iter()
                .filter(|f| f
                    .relative_path
                    .eq_ignore_ascii_case("luma/global/copy_ps.hlsl"))
                .count(),
            1
        );
    }

    #[test]
    fn case_differing_duplicate_entries_are_rejected() {
        let addon = addon_bytes();
        let zip = zip_with_entries(&[
            ("Luma-Game.addon", addon.as_slice()),
            ("Luma/Global/Copy_PS.hlsl", b"technique A {}"),
            ("luma/global/copy_ps.hlsl", b"technique B {}"),
        ]);
        assert!(extract_luma_payload(&zip, "Luma-Game.addon", Architecture::X64).is_err());
    }

    #[test]
    fn rejects_entries_exceeding_the_per_file_size_limit() {
        let addon = addon_bytes();
        let huge = vec![0u8; (MAX_ENTRY_UNCOMPRESSED_BYTES + 1) as usize];
        let zip = zip_with_entries(&[
            ("Luma-Game.addon", addon.as_slice()),
            ("Luma/Global/Huge.hlsl", huge.as_slice()),
        ]);
        assert!(extract_luma_payload(&zip, "Luma-Game.addon", Architecture::X64).is_err());
    }

    #[test]
    fn rejects_more_entries_than_the_archive_entry_cap() {
        let addon = addon_bytes();
        let mut entries: Vec<(String, Vec<u8>)> = vec![("Luma-Game.addon".to_owned(), addon)];
        for i in 0..MAX_ZIP_ENTRIES {
            entries.push((format!("Luma/Global/F{i}.hlsl"), b"x".to_vec()));
        }
        let borrowed: Vec<(&str, &[u8])> = entries
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
            .collect();
        let zip = zip_with_entries(&borrowed);
        assert!(extract_luma_payload(&zip, "Luma-Game.addon", Architecture::X64).is_err());
    }

    #[test]
    fn rejects_a_root_addon_that_does_not_match_manifest_identity() {
        let addon = addon_bytes();
        let zip = zip_with_entries(&[("Luma-Other.addon", addon.as_slice())]);

        assert!(
            extract_luma_payload(&zip, "Luma-Game.addon", Architecture::X64).is_err(),
            "a similarly named payload must not be installed under another title's identity"
        );
    }
}
