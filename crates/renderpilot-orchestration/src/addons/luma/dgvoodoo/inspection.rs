use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use renderpilot_domain::{InstalledAddon, TrackedSource, TrackedSourceRole, Version};

use super::super::types::{ExternalConfigSection, LumaExternalRequirement};
use super::model::{AdoptedDgVoodoo, ExistingDgVoodoo, OwnedDgVoodooStatus, ReusedDgVoodoo};
use super::plan::{config_sections, managed_config_default};

/// Returns the dgVoodoo requirement, when this title has one.
#[must_use]
pub(crate) fn requirement(
    requirement: Option<&LumaExternalRequirement>,
) -> Option<&LumaExternalRequirement> {
    requirement.filter(|r| matches!(r, LumaExternalRequirement::Dgvoodoo2 { .. }))
}

/// Advisory wrapper provenance for DB-loss recovery when the on-disk stack is
/// [`ExistingDgVoodoo::CompatibleAdoptable`]. Uses the current catalogue archive
/// pin (URL + SHA-256) without inventing ETag/HTTP history. Enough for
/// [`record_can_manage_runtime`] / freshness; update prepare still re-fetches
/// when the PE is outdated or incomplete.
#[must_use]
pub(crate) fn advisory_wrapper_source(requirement: &LumaExternalRequirement) -> TrackedSource {
    let LumaExternalRequirement::Dgvoodoo2 {
        version, source, ..
    } = requirement;
    TrackedSource::new(
        TrackedSourceRole::DgVoodooWrapper,
        source.url.clone(),
        None,
        source.sha256.clone(),
    )
    .with_channel(format!("dgvoodoo2@{version}"))
    .with_advisory()
}

/// Bare game-directory file names this dependency owns.
#[must_use]
pub(crate) fn game_file_names(requirement: &LumaExternalRequirement) -> Vec<&str> {
    match requirement {
        LumaExternalRequirement::Dgvoodoo2 {
            install_map,
            config_file,
            ..
        } => {
            let mut names: Vec<&str> = install_map
                .iter()
                .map(|entry| entry.dest.as_str())
                .collect();
            names.push(config_file.as_str());
            names
        }
    }
}

/// Common dgVoodoo wrapper / config basenames that Luma may own historically.
/// Used when the current profile no longer declares the dependency (catalogue
/// drift) so set-diff / payload-intact checks still treat them as dependencies.
const HISTORICAL_DEPENDENCY_BASENAMES: &[&str] = &[
    "D3D9.dll",
    "D3D8.dll",
    "D3DImm.dll",
    "DDraw.dll",
    "D3D11.dll",
    "D3D10.dll",
    "D3D10_1.dll",
    "dgVoodoo.conf",
    "dgVoodooCpl.exe",
    "dgVoodooCpl.conf",
];

#[must_use]
pub(crate) fn historical_dependency_basenames() -> &'static [&'static str] {
    HISTORICAL_DEPENDENCY_BASENAMES
}

/// Whether `path`'s file name looks like a managed dgVoodoo dependency file.
#[must_use]
pub(crate) fn is_dependency_basename(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    HISTORICAL_DEPENDENCY_BASENAMES
        .iter()
        .any(|known| name.eq_ignore_ascii_case(known))
}

/// Classifies an already present dgVoodoo runtime without downloading anything.
/// A partial footprint is intentionally a conflict: replacing just the missing
/// files would create a mixed runtime and overwrite a user installation.
#[must_use]
pub(crate) fn assess_existing(
    game_dir: &Path,
    requirement: &LumaExternalRequirement,
) -> ExistingDgVoodoo {
    let LumaExternalRequirement::Dgvoodoo2 {
        version,
        install_map,
        config_file,
        config,
        ..
    } = requirement;

    let existing = install_map
        .iter()
        .filter(|entry| game_dir.join(&entry.dest).exists())
        .count();
    if existing == 0 {
        return ExistingDgVoodoo::Absent;
    }
    if existing != install_map.len()
        || install_map
            .iter()
            .any(|entry| !game_dir.join(&entry.dest).is_file())
    {
        return ExistingDgVoodoo::Conflict(
            "dgVoodoo is incomplete: every DLL declared by this profile must be present".to_owned(),
        );
    }

    let Some(anchor) = install_map
        .iter()
        .find(|entry| entry.dest.eq_ignore_ascii_case("D3D9.dll"))
    else {
        return ExistingDgVoodoo::Conflict(
            "the dgVoodoo profile has no D3D9.dll identity anchor".to_owned(),
        );
    };
    let required = match normalized_requirement_version(version) {
        Some(version) => version,
        None => {
            return ExistingDgVoodoo::Conflict(
                "the dgVoodoo requirement has an invalid version".to_owned(),
            );
        }
    };
    let Some(inspection) = renderpilot_detection::inspect_pe(&game_dir.join(&anchor.dest)) else {
        return ExistingDgVoodoo::Conflict("D3D9.dll is not a readable PE file".to_owned());
    };
    if !is_compatible_inspection(&inspection, &required) {
        return ExistingDgVoodoo::Conflict(format!(
            "D3D9.dll is not a compatible dgVoodoo runtime at version {required} or newer"
        ));
    }

    if config_is_adoptable(&game_dir.join(config_file), config) {
        ExistingDgVoodoo::CompatibleAdoptable
    } else {
        ExistingDgVoodoo::CompatibleReusable
    }
}

/// Checks only the on-disk runtime version required for an already-owned
/// dgVoodoo stack. Unlike [`assess_existing`], an invalid result is not an
/// install conflict: the record may be stale or the DLL may have been changed
/// after adoption, so update reporting must stay conservative.
#[must_use]
pub(crate) fn owned_status(
    game_dir: &Path,
    requirement: &LumaExternalRequirement,
) -> OwnedDgVoodooStatus {
    let LumaExternalRequirement::Dgvoodoo2 {
        version,
        install_map,
        ..
    } = requirement;
    if install_map
        .iter()
        .any(|entry| !game_dir.join(&entry.dest).is_file())
    {
        return OwnedDgVoodooStatus::Incomplete;
    }
    let Some(anchor) = install_map
        .iter()
        .find(|entry| entry.dest.eq_ignore_ascii_case("D3D9.dll"))
    else {
        return OwnedDgVoodooStatus::Unknown;
    };
    let Some(required) = normalized_requirement_version(version) else {
        return OwnedDgVoodooStatus::Unknown;
    };
    let Some(inspection) = renderpilot_detection::inspect_pe(&game_dir.join(&anchor.dest)) else {
        return OwnedDgVoodooStatus::Unknown;
    };
    owned_status_from_inspection(&inspection, &required)
}

/// Whether a record owns every runtime DLL the current requirement names.
/// Configuration ownership is intentionally irrelevant here: a config may have
/// been absent at adoption and created by the first manifest merge, while DLL
/// ownership is the authority required to report and apply a runtime update.
///
/// Path equality uses [`crate::addons::luma::tracking::owns_path`] (`same_path`)
/// so Windows casing / separator / long-path form drift cannot drop ownership.
/// Full-map ownership check (stricter than [`record_can_manage_runtime`]).
/// Currently exercised by unit tests; production paths use the soft manage gate.
#[cfg(test)]
#[must_use]
pub(crate) fn record_owns_runtime(
    record: &InstalledAddon,
    game_dir: &Path,
    requirement: &LumaExternalRequirement,
) -> bool {
    let LumaExternalRequirement::Dgvoodoo2 { install_map, .. } = requirement;
    install_map.iter().all(|entry| {
        let expected = game_dir.join(&entry.dest);
        crate::addons::luma::tracking::owns_path(record, &expected)
    })
}

/// Whether this install may update/repair the managed dgVoodoo stack under the
/// current catalogue map — including when the map has grown since install.
///
/// Authority requires:
/// 1. A `DgVoodooWrapper` tracked source (user-reused runtimes have none).
/// 2. Ownership of a **non-empty subset** of current `install_map` dests.
/// 3. No **unowned existing file** on a map dest (would create a mixed runtime).
///
/// Missing map dests are allowed (create on update). Catalogue expansion that
/// only adds absent files therefore stays manageable; expansion onto a foreign
/// on-disk DLL is blocked.
#[must_use]
pub(crate) fn record_can_manage_runtime(
    record: &InstalledAddon,
    game_dir: &Path,
    requirement: &LumaExternalRequirement,
) -> bool {
    use renderpilot_domain::TrackedSourceRole;

    let LumaExternalRequirement::Dgvoodoo2 { install_map, .. } = requirement;
    if install_map.is_empty() {
        return false;
    }
    let has_wrapper_source = record
        .tracked_sources()
        .iter()
        .any(|source| source.role() == TrackedSourceRole::DgVoodooWrapper);
    if !has_wrapper_source {
        return false;
    }

    let mut owns_any = false;
    for entry in install_map {
        let path = game_dir.join(&entry.dest);
        if crate::addons::luma::tracking::owns_path(record, &path) {
            owns_any = true;
            continue;
        }
        // Unowned but present on disk: refuse to claim / clobber a foreign file.
        if path.is_file() {
            return false;
        }
    }
    owns_any
}

/// True when at least one current `install_map` dest is not yet owned by the
/// record (catalogue growth). Call only after [`record_can_manage_runtime`].
#[must_use]
pub(crate) fn map_needs_ownership_sync(
    record: &InstalledAddon,
    game_dir: &Path,
    requirement: &LumaExternalRequirement,
) -> bool {
    let LumaExternalRequirement::Dgvoodoo2 { install_map, .. } = requirement;
    install_map
        .iter()
        .any(|entry| !crate::addons::luma::tracking::owns_path(record, &game_dir.join(&entry.dest)))
}

/// True when the record owns at least one current `install_map` dest under
/// `game_dir` (path equality via [`crate::addons::luma::tracking::owns_path`]).
#[must_use]
pub(crate) fn record_owns_any_map_dest(
    record: &InstalledAddon,
    game_dir: &Path,
    requirement: &LumaExternalRequirement,
) -> bool {
    let LumaExternalRequirement::Dgvoodoo2 { install_map, .. } = requirement;
    install_map
        .iter()
        .any(|entry| crate::addons::luma::tracking::owns_path(record, &game_dir.join(&entry.dest)))
}

/// Returns the reusable configuration payload after [`assess_existing`] found
/// a compatible runtime. Kept separate so callers cannot accidentally reuse a
/// runtime without first performing the classification.
#[must_use]
pub(crate) fn reused_config(requirement: &LumaExternalRequirement) -> ReusedDgVoodoo {
    let LumaExternalRequirement::Dgvoodoo2 {
        config_file,
        config,
        ..
    } = requirement;
    ReusedDgVoodoo {
        config_file: config_file.clone(),
        config_default: managed_config_default(config),
        config_sections: config_sections(config),
    }
}

/// Returns the existing paths safe to own after [`assess_existing`] classified
/// this runtime as [`ExistingDgVoodoo::CompatibleAdoptable`]. The caller keeps
/// the host lifecycle gate separate: a standalone dgVoodoo setup is never
/// adopted merely because its config happens to be minimal.
#[must_use]
pub(crate) fn adopted_existing(
    requirement: &LumaExternalRequirement,
    game_dir: &Path,
) -> AdoptedDgVoodoo {
    let LumaExternalRequirement::Dgvoodoo2 {
        install_map,
        config_file,
        ..
    } = requirement;
    let mut existing_paths: Vec<PathBuf> = install_map
        .iter()
        .map(|entry| game_dir.join(&entry.dest))
        .collect();
    let config_path = game_dir.join(config_file);
    if config_path.is_file() {
        existing_paths.push(config_path);
    }
    AdoptedDgVoodoo {
        config: reused_config(requirement),
        existing_paths,
    }
}

pub(super) fn config_is_adoptable(path: &Path, expected: &[ExternalConfigSection]) -> bool {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return true,
        Err(_) => return false,
    };
    let ini = crate::addons::ini::Ini::parse(&text);
    if ini
        .preamble_lines()
        .iter()
        .any(|line| !is_ignorable_ini_line(line))
    {
        return false;
    }

    let mut seen_sections = HashSet::new();
    let mut seen_assignments = HashSet::new();
    for (section, lines) in ini.raw_sections() {
        let section_key = section.to_ascii_lowercase();
        if !seen_sections.insert(section_key.clone())
            || expected
                .iter()
                .all(|candidate| !candidate.section.eq_ignore_ascii_case(section))
        {
            return false;
        }
        for line in lines {
            if is_ignorable_ini_line(line) {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return false;
            };
            let key = key.trim();
            let value = value.trim();
            if key.is_empty()
                || !seen_assignments.insert((section_key.clone(), key.to_ascii_lowercase()))
                || !expected.iter().any(|candidate| {
                    candidate.section.eq_ignore_ascii_case(section)
                        && candidate.entries.iter().any(|entry| {
                            entry.key.eq_ignore_ascii_case(key) && entry.value == value
                        })
                })
            {
                return false;
            }
        }
    }
    true
}

fn is_ignorable_ini_line(line: &str) -> bool {
    let line = line.trim();
    line.is_empty() || line.starts_with(';') || line.starts_with('#')
}

pub(super) fn is_compatible_inspection(
    inspection: &renderpilot_detection::PeInspection,
    required: &Version,
) -> bool {
    dgvoodoo_product_version(inspection).is_some_and(|version| version.cmp(required).is_ge())
}

fn dgvoodoo_product_version(inspection: &renderpilot_detection::PeInspection) -> Option<Version> {
    let product_name_matches = inspection
        .identity
        .product_name
        .as_deref()
        .is_some_and(|name| name.eq_ignore_ascii_case("dgVoodoo"));
    product_name_matches.then_some(())?;
    inspection
        .identity
        .product_version
        .as_deref()
        .and_then(renderpilot_detection::parse_windows_version_text)
}

pub(super) fn owned_status_from_inspection(
    inspection: &renderpilot_detection::PeInspection,
    required: &Version,
) -> OwnedDgVoodooStatus {
    match dgvoodoo_product_version(inspection) {
        Some(installed) if installed.cmp(required).is_ge() => OwnedDgVoodooStatus::Current,
        Some(_) => OwnedDgVoodooStatus::Outdated,
        None => OwnedDgVoodooStatus::Unknown,
    }
}

/// Converts the public dgVoodoo release label to the product-version layout
/// stored in its PE resource. For example upstream release `2.87.3` is
/// reported by `D3D9.dll` as `2.8.7.3`.
pub(super) fn normalized_requirement_version(value: &str) -> Option<Version> {
    let parts: Vec<&str> = value.trim().split('.').collect();
    let normalized = match parts.as_slice() {
        [major, minor, patch]
            if major.bytes().all(|byte| byte.is_ascii_digit())
                && minor.len() == 2
                && minor.bytes().all(|byte| byte.is_ascii_digit())
                && patch.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            format!("{major}.{}.{}.{}", &minor[0..1], &minor[1..2], patch)
        }
        _ => value.trim().to_owned(),
    };
    Version::parse(normalized).ok()
}
