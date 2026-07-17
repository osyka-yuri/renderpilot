//! External-requirement (dgVoodoo) validation for Luma titles.

use std::collections::HashSet;

use renderpilot_domain::GraphicsApi;

use crate::ServiceError;

use super::super::errors;
use super::super::types::{
    ExternalConfigEntry, ExternalConfigSection, LumaExternalRequirement, ManagedArchiveSource,
    ManagedInstallMapEntry,
};
use crate::addons::manifest_validate::{
    ensure_not_blank, ensure_safe_file_name, ensure_semver, is_lowercase_sha256_hex,
};
pub(super) fn validate_external_requirement(
    title_id: &str,
    requirement: &LumaExternalRequirement,
) -> Result<(), ServiceError> {
    match requirement {
        LumaExternalRequirement::Dgvoodoo2 {
            version,
            accepted_detected_apis,
            reshade_proxy_dll,
            source,
            install_map,
            config_file,
            config,
        } => {
            ensure_semver(&format!("title `{title_id}` dgVoodoo2"), "version", version)?;
            validate_accepted_detected_apis(title_id, accepted_detected_apis)?;
            validate_proxy_dll(title_id, reshade_proxy_dll)?;
            validate_managed_source(title_id, source)?;
            validate_install_map(title_id, install_map)?;
            ensure_safe_file_name("external requirement config_file", config_file)?;
            ensure_no_install_target_conflict(
                title_id,
                install_map,
                config_file,
                reshade_proxy_dll,
            )?;
            validate_external_config(title_id, config)?;
        }
    }
    Ok(())
}

fn validate_accepted_detected_apis(
    title_id: &str,
    apis: &[GraphicsApi],
) -> Result<(), ServiceError> {
    if apis.is_empty() {
        return Err(errors::failed(format!(
            "title `{title_id}` external requirement must accept at least one detected API"
        )));
    }
    let mut seen = Vec::with_capacity(apis.len());
    for api in apis {
        if !matches!(
            api,
            GraphicsApi::D3D9 | GraphicsApi::D3D10 | GraphicsApi::D3D11 | GraphicsApi::D3D12
        ) {
            return Err(errors::failed(format!(
                "title `{title_id}` external requirement accepts non-DirectX API `{api}`"
            )));
        }
        if seen.contains(api) {
            return Err(errors::failed(format!(
                "title `{title_id}` external requirement repeats accepted API `{api}`"
            )));
        }
        seen.push(*api);
    }
    Ok(())
}

fn validate_proxy_dll(title_id: &str, proxy_dll: &str) -> Result<(), ServiceError> {
    let normalized = proxy_dll.trim().to_ascii_lowercase();
    if !matches!(
        normalized.as_str(),
        "dxgi.dll" | "d3d9.dll" | "d3d10.dll" | "d3d11.dll" | "d3d12.dll"
    ) {
        return Err(errors::failed(format!(
            "title `{title_id}` external requirement reshade_proxy_dll `{proxy_dll}` is not a supported ReShade proxy slot"
        )));
    }
    Ok(())
}

fn validate_managed_source(
    title_id: &str,
    source: &ManagedArchiveSource,
) -> Result<(), ServiceError> {
    if !source.url.starts_with("https://") {
        return Err(errors::failed(format!(
            "title `{title_id}` external requirement source url must be HTTPS"
        )));
    }
    if !is_lowercase_sha256_hex(&source.sha256) {
        return Err(errors::failed(format!(
            "title `{title_id}` external requirement source sha256 must be lowercase hex SHA-256"
        )));
    }
    if source.size == 0 {
        return Err(errors::failed(format!(
            "title `{title_id}` external requirement source size must be greater than zero"
        )));
    }
    Ok(())
}

fn validate_install_map(
    title_id: &str,
    install_map: &[ManagedInstallMapEntry],
) -> Result<(), ServiceError> {
    if install_map.is_empty() {
        return Err(errors::failed(format!(
            "title `{title_id}` external requirement install_map must not be empty"
        )));
    }
    let mut sources = HashSet::with_capacity(install_map.len());
    let mut dests = HashSet::with_capacity(install_map.len());
    for entry in install_map {
        ensure_safe_archive_path("external requirement install_map source", &entry.source)?;
        ensure_safe_file_name("external requirement install_map dest", &entry.dest)?;
        if !is_lowercase_sha256_hex(&entry.sha256) {
            return Err(errors::failed(format!(
                "title `{title_id}` external requirement install_map `{}` sha256 must be lowercase hex SHA-256",
                entry.source
            )));
        }
        if entry.size == 0 {
            return Err(errors::failed(format!(
                "title `{title_id}` external requirement install_map `{}` size must be greater than zero",
                entry.source
            )));
        }
        let source_key = entry.source.to_ascii_lowercase();
        if !sources.insert(source_key) {
            return Err(errors::failed(format!(
                "title `{title_id}` external requirement install_map repeats source `{}`",
                entry.source
            )));
        }
        let dest_key = entry.dest.to_ascii_lowercase();
        if !dests.insert(dest_key) {
            return Err(errors::failed(format!(
                "title `{title_id}` external requirement install_map repeats dest `{}`",
                entry.dest
            )));
        }
    }
    Ok(())
}

fn ensure_no_install_target_conflict(
    title_id: &str,
    install_map: &[ManagedInstallMapEntry],
    config_file: &str,
    reshade_proxy_dll: &str,
) -> Result<(), ServiceError> {
    if install_map
        .iter()
        .any(|entry| entry.dest.eq_ignore_ascii_case(config_file))
    {
        return Err(errors::failed(format!(
            "title `{title_id}` external requirement config_file `{config_file}` conflicts with install_map target"
        )));
    }
    // Proxy host and dgVoodoo wrapper DLLs both land beside the exe; a
    // curated profile must not schedule two writes to the same path.
    if install_map
        .iter()
        .any(|entry| entry.dest.eq_ignore_ascii_case(reshade_proxy_dll))
    {
        return Err(errors::failed(format!(
            "title `{title_id}` external requirement reshade_proxy_dll `{reshade_proxy_dll}` conflicts with install_map target"
        )));
    }
    Ok(())
}

fn ensure_safe_archive_path(field: &str, value: &str) -> Result<(), ServiceError> {
    ensure_not_blank(field, value)?;
    if value.contains('\\')
        || value.starts_with('/')
        || value
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(errors::failed(format!(
            "`{field}` must be a safe relative archive path, got `{value}`"
        )));
    }
    Ok(())
}

fn validate_external_config(
    title_id: &str,
    config: &[ExternalConfigSection],
) -> Result<(), ServiceError> {
    if config.is_empty() {
        return Err(errors::failed(format!(
            "title `{title_id}` external requirement config must not be empty"
        )));
    }
    for section in config {
        validate_external_config_section(title_id, section)?;
    }
    Ok(())
}

fn validate_external_config_section(
    title_id: &str,
    section: &ExternalConfigSection,
) -> Result<(), ServiceError> {
    ensure_not_blank("external config section", &section.section)?;
    if section.entries.is_empty() {
        return Err(errors::failed(format!(
            "title `{title_id}` external config section `{}` must contain entries",
            section.section
        )));
    }
    for entry in &section.entries {
        validate_external_config_entry(title_id, &section.section, entry)?;
    }
    Ok(())
}

fn validate_external_config_entry(
    title_id: &str,
    section: &str,
    entry: &ExternalConfigEntry,
) -> Result<(), ServiceError> {
    ensure_not_blank("external config key", &entry.key)?;
    ensure_not_blank("external config value", &entry.value)?;
    if entry.key.contains('\r')
        || entry.key.contains('\n')
        || entry.value.contains('\r')
        || entry.value.contains('\n')
    {
        return Err(errors::failed(format!(
            "title `{title_id}` external config entry `{}.{}` must be single-line",
            section, entry.key
        )));
    }
    Ok(())
}
