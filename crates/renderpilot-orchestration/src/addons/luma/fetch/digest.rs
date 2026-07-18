//! Stable recovery identity for the Luma-owned payload tree.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::ServiceError;

use super::super::errors;
use super::types::LumaPayload;

/// Stable identity of the Luma-owned portion of a payload: the exact root
/// add-on and the `Luma/**` tree. This deliberately excludes optional root
/// files such as `nvngx_dlss.dll`, which can shadow a game-owned file and
/// cannot be safely adopted after the install database is lost.
///
/// Normal installs keep the ZIP digest. DB-loss recovery instead stores this
/// content digest as an advisory source: it proves what is on disk without
/// pretending that a local payload file is the release ZIP.
#[must_use]
pub(crate) fn recovery_payload_digest(payload: &LumaPayload) -> String {
    digest_payload_entries(
        payload
            .files
            .iter()
            .filter(|file| {
                file.relative_path
                    .eq_ignore_ascii_case(&payload.main_addon_rel)
                    || is_luma_payload_path(&file.relative_path)
            })
            .map(|file| (file.relative_path.as_str(), file.bytes.as_slice())),
    )
}

/// Builds the same identity from files already on disk during DB-loss
/// recovery. `created_files` may also include adopted ReShade/dgVoodoo paths;
/// only the exact add-on and `Luma/**` files participate.
pub(crate) fn recovery_payload_digest_from_disk(
    addon_file: &Path,
    created_files: &[PathBuf],
) -> Result<String, ServiceError> {
    let addon_name = addon_file
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            errors::failed(format!(
                "adopted Luma add-on path has no valid file name: {}",
                addon_file.display()
            ))
        })?;
    let luma_dir = addon_file
        .parent()
        .map(|parent| parent.join("Luma"))
        .ok_or_else(|| {
            errors::failed(format!(
                "adopted Luma add-on path has no parent directory: {}",
                addon_file.display()
            ))
        })?;

    let mut entries = Vec::new();
    for path in created_files {
        let relative = if path == addon_file {
            Some(addon_name.to_owned())
        } else {
            path.strip_prefix(&luma_dir).ok().and_then(|relative| {
                relative
                    .to_str()
                    .map(|relative| format!("Luma/{}", relative.replace('\\', "/")))
            })
        };
        let Some(relative) = relative else {
            continue;
        };
        let bytes = std::fs::read(path).map_err(|error| {
            errors::failed(format!(
                "failed to read adopted Luma payload file `{}`: {error}",
                path.display()
            ))
        })?;
        entries.push((relative, bytes));
    }

    if !entries
        .iter()
        .any(|(relative, _)| relative.eq_ignore_ascii_case(addon_name))
    {
        return Err(errors::failed(format!(
            "adopted Luma payload is missing its exact add-on `{addon_name}`"
        )));
    }

    Ok(digest_payload_entries(entries.iter().map(
        |(relative, bytes)| (relative.as_str(), bytes.as_slice()),
    )))
}

fn is_luma_payload_path(relative: &str) -> bool {
    relative
        .replace('\\', "/")
        .to_ascii_lowercase()
        .starts_with("luma/")
}

fn digest_payload_entries<'a>(entries: impl Iterator<Item = (&'a str, &'a [u8])>) -> String {
    let mut entries: Vec<_> = entries.collect();
    entries.sort_unstable_by_key(|(relative, _)| relative.to_ascii_lowercase());

    let mut digest = Sha256::new();
    for (relative, bytes) in entries {
        let relative = relative.to_ascii_lowercase();
        digest.update((relative.len() as u64).to_le_bytes());
        digest.update(relative.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::addons::luma::fetch::types::LumaPayloadFile;

    #[test]
    fn recovery_digest_compares_only_the_provable_luma_payload_tree() {
        let dir = tempfile::tempdir().expect("tempdir");
        let addon = dir.path().join("Luma-Game.addon");
        let shader = dir.path().join("Luma/Global/Copy_PS.hlsl");
        let dlss = dir.path().join("nvngx_dlss.dll");
        let foreign_runtime = dir.path().join("dxgi.dll");
        fs::create_dir_all(shader.parent().expect("shader parent")).expect("shader dir");
        fs::write(&addon, b"addon").expect("addon");
        fs::write(&shader, b"shader").expect("shader");
        fs::write(&dlss, b"game-owned-dlss").expect("dlss");
        fs::write(&foreign_runtime, b"reshade").expect("runtime");

        let payload = LumaPayload {
            files: vec![
                LumaPayloadFile {
                    relative_path: "Luma-Game.addon".to_owned(),
                    bytes: b"addon".to_vec(),
                },
                LumaPayloadFile {
                    relative_path: "foreign.addon".to_owned(),
                    bytes: b"foreign".to_vec(),
                },
                LumaPayloadFile {
                    relative_path: "Luma/Global/Copy_PS.hlsl".to_owned(),
                    bytes: b"shader".to_vec(),
                },
                LumaPayloadFile {
                    relative_path: "nvngx_dlss.dll".to_owned(),
                    bytes: b"luma-dlss".to_vec(),
                },
            ],
            main_addon_rel: "Luma-Game.addon".to_owned(),
            zip_digest: "zip".to_owned(),
            etag: None,
            last_modified: None,
            build_number: None,
        };

        let disk_digest = recovery_payload_digest_from_disk(
            &addon,
            &[addon.clone(), shader.clone(), dlss, foreign_runtime],
        )
        .expect("disk digest");
        assert_eq!(disk_digest, recovery_payload_digest(&payload));

        let mut different_foreign_addon = payload.clone();
        different_foreign_addon.files[1].bytes = b"changed foreign".to_vec();
        assert_eq!(
            recovery_payload_digest(&different_foreign_addon),
            recovery_payload_digest(&payload),
            "only the exact manifest add-on participates in recovery identity"
        );

        fs::write(&shader, b"changed shader").expect("change shader");
        assert_ne!(
            recovery_payload_digest_from_disk(&addon, &[addon.clone(), shader])
                .expect("changed disk digest"),
            recovery_payload_digest(&payload)
        );
    }
}
