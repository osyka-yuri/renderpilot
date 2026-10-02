//! Pure projection of Proxy ownership receipts that outlive a missing root.

use std::collections::HashSet;

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::{
    InstallRoot, InstalledAddon, InstalledAddonParts, ManagedAddonFile, PathRef, TrackedSourceRole,
    normalized_path_key,
};

/// The host DLL names supported by the existing Proxy install records.
const PROXY_HOST_FILE_NAMES: &[&str] = &[
    "d3d9.dll",
    "d3d10.dll",
    "d3d10_1.dll",
    "d3d11.dll",
    "d3d12.dll",
    "dxgi.dll",
    "opengl32.dll",
    "reshade32.dll",
    "reshade64.dll",
];

/// Keeps only the durable ownership closure outside a confirmed-lost root.
/// This is a metadata projection: it never checks or changes the filesystem.
pub(super) fn project_external_addon_closure(
    addon: &InstalledAddon,
    install_root: &InstallRoot,
) -> AppResult<Option<InstalledAddon>> {
    let primary_is_local = path_is_contained(addon.addon_file(), install_root, "add-on payload")?;
    let mut created_files = Vec::with_capacity(addon.created_files().len());
    for path in addon.created_files() {
        if path_is_contained(path, install_root, "created-file receipt")? {
            continue;
        }
        created_files.push(path.clone());
    }

    let mut backed_up_files = Vec::with_capacity(addon.backed_up_files().len());
    for path in addon.backed_up_files() {
        if path_is_contained(path, install_root, "backup receipt")? {
            continue;
        }
        backed_up_files.push(path.clone());
    }

    let mut managed_files = Vec::with_capacity(addon.managed_files().len());
    for file in addon.managed_files() {
        if path_is_contained(file.path(), install_root, "managed-file binding")? {
            continue;
        }
        managed_files.push(file.clone());
    }

    let renodx_config_receipt = match addon.renodx_config_receipt() {
        Some(receipt)
            if path_is_contained(&receipt.ini_path, install_root, "RenoDX config receipt")? =>
        {
            None
        }
        Some(receipt) => Some(receipt.clone()),
        None => None,
    };

    let registered_exe_path = match addon.registered_exe_path() {
        Some(path) if path_is_contained(path, install_root, "registered executable")? => None,
        Some(path) => Some(path.clone()),
        None => None,
    };

    let has_local_host_receipts = addon
        .tracked_sources()
        .iter()
        .any(|source| source.role() == TrackedSourceRole::HostBinary)
        && proxy_host_receipts_are_contained(addon, install_root)?;
    let tracked_sources = addon
        .tracked_sources()
        .iter()
        .filter(|source| source.role() != TrackedSourceRole::HostBinary || !has_local_host_receipts)
        .cloned()
        .collect();
    let has_external_ownership = !created_files.is_empty()
        || !backed_up_files.is_empty()
        || !managed_files.is_empty()
        || renodx_config_receipt.is_some()
        || registered_exe_path.is_some();

    if primary_is_local {
        if has_external_ownership {
            return Err(collection_conflict(
                "the primary add-on payload is root-local while another receipt is external",
            ));
        }
        return Ok(None);
    }

    // The domain requires addon_file to appear in created_files. A malformed
    // durable record must not be silently repaired by local collection.
    if !created_files.contains(addon.addon_file()) {
        return Err(collection_conflict(
            "the external primary payload is absent from the created-file receipts",
        ));
    }

    let mut projected = InstalledAddon::from_parts_with_managed(InstalledAddonParts {
        game_id: addon.game_id().clone(),
        kind: addon.kind(),
        addon_file: addon.addon_file().clone(),
        addon_version: addon.addon_version().map(str::to_owned),
        created_files,
        backed_up_files,
        managed_files,
        tracked_sources,
        renodx_config_receipt,
        engine_config_journal: addon.engine_config_journal().cloned(),
    })
    .map_err(|error| {
        collection_conflict(format_args!("invalid projected add-on receipt: {error}"))
    })?
    .ok_or_else(|| collection_conflict("the projected primary payload receipt is incomplete"))?
    .with_timestamps(addon.installed_at(), addon.updated_at());

    if let Some(host_kind) = addon.host_kind() {
        projected = projected.with_host_kind(host_kind);
    }
    if let Some(channel) = addon.reshade_channel() {
        projected = projected.with_reshade_channel(channel);
    }
    if let Some(path) = registered_exe_path {
        projected = projected.with_registered_exe_path(path);
    }
    // `InstalledAddonParts` validates and preserves the typed config and
    // independent Engine projection. The latter remains in its standalone table.
    Ok(Some(projected))
}

fn proxy_host_receipts_are_contained(
    addon: &InstalledAddon,
    install_root: &InstallRoot,
) -> AppResult<bool> {
    let mut distinct_candidates = HashSet::new();
    let mut containment = None;
    for path in addon
        .created_files()
        .iter()
        .chain(addon.backed_up_files())
        .chain(addon.managed_files().iter().map(ManagedAddonFile::path))
    {
        let name = path.as_str().rsplit('/').next().unwrap_or_default();
        if !PROXY_HOST_FILE_NAMES
            .iter()
            .any(|candidate| name.eq_ignore_ascii_case(candidate))
        {
            continue;
        }

        let is_contained = path_is_contained(path, install_root, "Proxy host receipt")?;
        if !distinct_candidates.insert(normalized_path_key(path.as_str())) {
            continue;
        }
        if containment.is_some_and(|known| known != is_contained) {
            return Err(collection_conflict(
                "Proxy host receipts disagree about installation-root containment",
            ));
        }
        containment = Some(is_contained);
    }

    containment.ok_or_else(|| {
        collection_conflict("HostBinary provenance has no recognized durable Proxy host receipt")
    })
}

fn path_is_contained(path: &PathRef, install_root: &InstallRoot, field: &str) -> AppResult<bool> {
    if !is_well_formed_absolute_path(path.as_str()) {
        return Err(collection_conflict(format_args!(
            "{field} has an ambiguous or non-absolute path"
        )));
    }
    Ok(install_root.contains_path(path))
}

fn is_well_formed_absolute_path(path: &str) -> bool {
    let slash_path = path.replace('\\', "/");
    let canonical = if let Some(rest) = strip_prefix_ignore_ascii_case(&slash_path, "//?/unc/") {
        format!("//{rest}")
    } else if let Some(rest) = slash_path.strip_prefix("//?/") {
        rest.to_owned()
    } else {
        slash_path
    };

    if canonical.starts_with("//./") || canonical.contains('\0') {
        return false;
    }

    let mut components = if let Some(rest) = canonical.strip_prefix("//") {
        let mut components = rest.split('/');
        let Some(server) = components.next() else {
            return false;
        };
        let Some(share) = components.next() else {
            return false;
        };
        if !is_valid_path_component(server) || !is_valid_path_component(share) {
            return false;
        }
        components
    } else if canonical.len() >= 3
        && canonical.as_bytes()[0].is_ascii_alphabetic()
        && canonical.as_bytes()[1] == b':'
        && canonical.as_bytes()[2] == b'/'
    {
        canonical[3..].split('/')
    } else if let Some(rest) = canonical.strip_prefix('/') {
        if rest.starts_with('/') {
            return false;
        }
        rest.split('/')
    } else {
        return false;
    };

    components.all(is_valid_path_component)
}

fn is_valid_path_component(component: &str) -> bool {
    !component.is_empty()
        && component != "."
        && component != ".."
        && !component.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
        })
}

fn strip_prefix_ignore_ascii_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .map(|_| &value[prefix.len()..])
}

fn collection_conflict(reason: impl std::fmt::Display) -> AppError {
    AppError::storage_failed(format!(
        "cannot collect absent installation add-on ownership safely: {reason}"
    ))
}

#[cfg(test)]
mod tests {
    use super::is_well_formed_absolute_path;

    #[test]
    fn absolute_path_validation_preserves_supported_forms_and_rejects_ambiguous_components() {
        let cases = [
            ("C:/Games/Example", true),
            (r"Z:\Games\Example", true),
            ("//server/share/Game", true),
            (r"\\server\share\Game", true),
            ("//?/C:/Games/Example", true),
            (r"\\?\UNC\server\share\Game", true),
            ("/opt/games/example", true),
            ("C:/", false),
            ("C://Games/Example", false),
            ("//server", false),
            ("//server/share/", false),
            ("/", false),
            ("//./pipe/name", false),
            ("/games/../outside", false),
            ("//server/share/na?me", false),
            ("abcdef☃/path", false),
        ];

        for (path, expected) in cases {
            assert_eq!(
                is_well_formed_absolute_path(path),
                expected,
                "unexpected validation result for {path:?}"
            );
        }
    }
}
