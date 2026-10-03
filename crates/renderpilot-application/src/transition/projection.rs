//! Member projection, legacy install names, and technology-specific removal policies.

use std::collections::HashSet;

use renderpilot_domain::{
    ComponentFile, LibraryArtifact, LibraryComponent, LibraryTechnology, fsr, xiph,
};

use crate::{
    AppError, AppResult,
    dxc::{COMPILER_FILE_NAME, VALIDATOR_FILE_NAME},
};

use super::xiph::project_xiph_members;

/// Resolves the artifact members that one concrete component transition writes.
///
/// Most technologies install the complete artifact. Streamline and DXC
/// packages are intersected by installed file name; Xiph packages are
/// intersected by semantic member. A swap therefore never expands the
/// integration chosen by the game.
pub fn resolve_transition_members<'a>(
    component: &LibraryComponent,
    artifact: &'a LibraryArtifact,
) -> AppResult<Vec<&'a ComponentFile>> {
    if component.technology() != artifact.technology() {
        return Err(AppError::invalid_input(
            "component and artifact technologies do not match",
        ));
    }

    let members = match artifact.technology() {
        LibraryTechnology::NvidiaStreamline => {
            let installed = installed_file_names(component)?;
            project_package_members(component, artifact, &installed, component.files().len() > 1)?
        }
        LibraryTechnology::MicrosoftDxc => {
            let installed = installed_file_names(component)?;
            require_dxc_component_shape(&installed)?;
            project_package_members(component, artifact, &installed, true)?
        }
        LibraryTechnology::XiphVorbis => project_xiph_members(component, artifact)?,
        _ => {
            let members: Vec<_> = artifact.files().iter().collect();
            require_unique_resolved_targets(component, &members)?;
            members
        }
    };
    if members.is_empty() {
        return Err(AppError::invalid_input(
            "artifact has no installable files for this component",
        ));
    }

    Ok(members)
}

/// Resolves the concrete basename written by one transition member.
///
/// Preserves canonical Xiph names. Vendor layouts require
/// [`crate::resolve_transition`] with an external-import proof.
#[must_use]
pub fn resolve_transition_install_target(
    component: &LibraryComponent,
    artifact_file: &ComponentFile,
) -> String {
    if component.technology() == LibraryTechnology::XiphVorbis
        && let Some(artifact_name) = artifact_file
            .install_as()
            .or_else(|| artifact_file.path().file_name())
        && let Some((artifact_member, _)) = xiph::classify_canonical_file_name(artifact_name)
        && let Some(installed_name) = component.files().iter().find_map(|file| {
            let name = file.path().file_name()?;
            let runtime = xiph::parse_runtime_file_name(name).ok().flatten()?;
            (!runtime.is_vendor() && runtime.member() == artifact_member).then_some(name)
        })
    {
        return installed_name.to_owned();
    }

    fsr::resolve_artifact_install_target(artifact_file, component.files())
}

/// Resolves installed files that a transition must remove in addition to its
/// writes.
///
/// A unified FSR backend supersedes stale split upscaling members, while
/// separately owned optional effects remain untouched. Callers supply the
/// already-resolved write targets so cleanup and installation cannot claim the
/// same path.
#[must_use]
pub fn resolve_transition_removals<'a, 'b>(
    removal_basis: &'a [ComponentFile],
    artifact: &LibraryArtifact,
    resolved_install_targets: impl IntoIterator<Item = &'b str>,
) -> Vec<&'a ComponentFile> {
    let target_is_unified_fsr = artifact.technology().family() == LibraryTechnology::AmdFsr
        && !fsr::is_split_marker(artifact.file_name());
    if !target_is_unified_fsr || !fsr::has_entry_point(removal_basis) {
        return Vec::new();
    }

    let planned_names: HashSet<String> = resolved_install_targets
        .into_iter()
        .map(str::to_ascii_lowercase)
        .collect();

    removal_basis
        .iter()
        .filter(|file| {
            file.path().file_name().is_some_and(|name| {
                fsr::is_upscaling_member(name)
                    && !planned_names.contains(&name.to_ascii_lowercase())
            })
        })
        .collect()
}

fn installed_file_names(component: &LibraryComponent) -> AppResult<HashSet<String>> {
    let mut names = HashSet::with_capacity(component.files().len());
    for file in component.files() {
        let name = file
            .path()
            .file_name()
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| AppError::invalid_input("component target has no file name"))?;
        if !names.insert(name) {
            return Err(AppError::invalid_input(
                "component has duplicate installed file targets",
            ));
        }
    }
    Ok(names)
}

fn project_package_members<'a>(
    component: &LibraryComponent,
    artifact: &'a LibraryArtifact,
    installed: &HashSet<String>,
    require_full_coverage: bool,
) -> AppResult<Vec<&'a ComponentFile>> {
    let mut package_names = HashSet::with_capacity(artifact.files().len());
    let mut projected = Vec::new();

    for member in artifact.files() {
        let install_name =
            fsr::resolve_artifact_install_target(member, component.files()).to_ascii_lowercase();
        if install_name.trim().is_empty() {
            return Err(AppError::invalid_input(
                "artifact resolves an empty install target",
            ));
        }
        let is_installed = installed.contains(&install_name);
        if let Some(duplicate) = package_names.replace(install_name) {
            return Err(AppError::invalid_input(format!(
                "package has duplicate install target: {duplicate}"
            )));
        }
        if is_installed {
            projected.push(member);
        }
    }

    if require_full_coverage {
        require_package_coverage(installed, &package_names, artifact.technology().as_slug())?;
    }

    Ok(projected)
}

fn require_package_coverage(
    installed: &HashSet<String>,
    package_names: &HashSet<String>,
    technology: &str,
) -> AppResult<()> {
    let mut missing: Vec<&str> = installed
        .iter()
        .filter(|name| !package_names.contains(*name))
        .map(String::as_str)
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    missing.sort_unstable();
    Err(AppError::invalid_input(format!(
        "{technology} package does not cover installed files: {}",
        missing.join(", ")
    )))
}

fn require_dxc_component_shape(installed: &HashSet<String>) -> AppResult<()> {
    let has_compiler = installed.contains(COMPILER_FILE_NAME);
    let has_valid_size =
        installed.len() == 1 || (installed.len() == 2 && installed.contains(VALIDATOR_FILE_NAME));

    if has_compiler && has_valid_size {
        Ok(())
    } else {
        Err(AppError::invalid_input(format!(
            "DXC component must contain {COMPILER_FILE_NAME}, optionally paired with \
             {VALIDATOR_FILE_NAME}"
        )))
    }
}

fn require_unique_resolved_targets(
    component: &LibraryComponent,
    members: &[&ComponentFile],
) -> AppResult<()> {
    let mut targets = HashSet::with_capacity(members.len());
    for member in members {
        let target =
            fsr::resolve_artifact_install_target(member, component.files()).to_ascii_lowercase();
        if target.trim().is_empty() {
            return Err(AppError::invalid_input(
                "artifact resolves an empty install target",
            ));
        }
        if let Some(duplicate) = targets.replace(target) {
            return Err(AppError::invalid_input(format!(
                "artifact resolves multiple members to install target: {duplicate}"
            )));
        }
    }
    Ok(())
}
