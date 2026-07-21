//! Pure component-aware selection of artifact members installed by a swap.

use std::collections::HashSet;

use renderpilot_domain::{
    ComponentFile, GraphicsComponent, GraphicsTechnology, LibraryArtifact, fsr,
};

use crate::{
    AppError, AppResult,
    dxc::{COMPILER_FILE_NAME, VALIDATOR_FILE_NAME},
};

/// Resolves the artifact members that one concrete component transition writes.
///
/// Most technologies install the complete artifact. Streamline and DXC
/// packages are intersected with the component's installed file set so a swap
/// never expands the integration chosen by the game.
pub fn resolve_transition_members<'a>(
    component: &GraphicsComponent,
    artifact: &'a LibraryArtifact,
) -> AppResult<Vec<&'a ComponentFile>> {
    if component.technology() != artifact.technology() {
        return Err(AppError::invalid_input(
            "component and artifact technologies do not match",
        ));
    }

    let members = match artifact.technology() {
        GraphicsTechnology::NvidiaStreamline => {
            let installed = installed_file_names(component)?;
            project_package_members(component, artifact, &installed, component.files().len() > 1)?
        }
        GraphicsTechnology::MicrosoftDxc => {
            let installed = installed_file_names(component)?;
            require_dxc_component_shape(&installed)?;
            project_package_members(component, artifact, &installed, true)?
        }
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
    let target_is_unified_fsr = artifact.technology().family() == GraphicsTechnology::AmdFsr
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

fn installed_file_names(component: &GraphicsComponent) -> AppResult<HashSet<String>> {
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
    component: &GraphicsComponent,
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
        if !package_names.insert(install_name.clone()) {
            return Err(AppError::invalid_input(format!(
                "package has duplicate install target: {install_name}"
            )));
        }
        if installed.contains(&install_name) {
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
    component: &GraphicsComponent,
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
        if !targets.insert(target.clone()) {
            return Err(AppError::invalid_input(format!(
                "artifact resolves multiple members to install target: {target}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentId, ComponentKind, GameId, PathRef, Sha256Hash,
        Swappability,
    };

    use super::*;

    fn file(path: &str, hash: char) -> ComponentFile {
        ComponentFile::new(PathRef::new(path).expect("path"))
            .with_sha256(Sha256Hash::new(hash.to_string().repeat(64)).expect("hash"))
    }

    fn streamline_component(names: &[&str]) -> GraphicsComponent {
        names.iter().fold(
            GraphicsComponent::new(
                ComponentId::new("component:streamline-transition").expect("component"),
                GameId::new("game:streamline-transition").expect("game"),
                ComponentKind::NativeLibrary,
                GraphicsTechnology::NvidiaStreamline,
                Swappability::BundleOnly,
            ),
            |component, name| component.with_file(file(&format!("C:/Game/{name}"), 'f')),
        )
    }

    fn streamline_artifact(names: &[&str]) -> LibraryArtifact {
        let files = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                file(
                    &format!("C:/Library/{name}"),
                    char::from(b'a' + index as u8),
                )
            })
            .collect();
        LibraryArtifact::new(
            ArtifactId::new("artifact:streamline-transition").expect("artifact"),
            GraphicsTechnology::NvidiaStreamline,
            names[0],
            files,
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
    }

    fn dxc_component(names: &[&str]) -> GraphicsComponent {
        names.iter().fold(
            GraphicsComponent::new(
                ComponentId::new("component:dxc-transition").expect("component"),
                GameId::new("game:dxc-transition").expect("game"),
                ComponentKind::NativeLibrary,
                GraphicsTechnology::MicrosoftDxc,
                if names.len() > 1 {
                    Swappability::BundleOnly
                } else {
                    Swappability::Swappable
                },
            ),
            |component, name| component.with_file(file(&format!("C:/Game/{name}"), 'f')),
        )
    }

    fn dxc_package() -> LibraryArtifact {
        LibraryArtifact::new(
            ArtifactId::new("artifact:dxc-transition").expect("artifact"),
            GraphicsTechnology::MicrosoftDxc,
            COMPILER_FILE_NAME,
            vec![
                file(&format!("C:/Library/{COMPILER_FILE_NAME}"), 'a'),
                file(&format!("C:/Library/{VALIDATOR_FILE_NAME}"), 'b'),
            ],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
    }

    #[test]
    fn streamline_transition_uses_only_installed_targets_and_requires_coverage() {
        let component = streamline_component(&["sl.common.dll", "sl.interposer.dll"]);
        let complete = streamline_artifact(&["sl.common.dll", "sl.dlss.dll", "sl.interposer.dll"]);
        let members = resolve_transition_members(&component, &complete).expect("transition");
        assert_eq!(
            members
                .iter()
                .filter_map(|file| file.path().file_name())
                .collect::<Vec<_>>(),
            ["sl.common.dll", "sl.interposer.dll"]
        );

        let incomplete = streamline_artifact(&["sl.common.dll", "sl.dlss.dll"]);
        assert!(
            resolve_transition_members(&component, &incomplete)
                .expect_err("coverage")
                .message()
                .contains("sl.interposer.dll")
        );
    }

    #[test]
    fn streamline_transition_rejects_an_empty_intersection() {
        let error = resolve_transition_members(
            &streamline_component(&["sl.common.dll"]),
            &streamline_artifact(&["sl.dlss.dll"]),
        )
        .expect_err("empty transition");
        assert!(error.message().contains("no installable files"));
    }

    #[test]
    fn dxc_transition_keeps_a_standalone_compiler_standalone() {
        let component = dxc_component(&[COMPILER_FILE_NAME]);
        let package = dxc_package();

        let members = resolve_transition_members(&component, &package).expect("transition");
        assert_eq!(members.len(), 1);
        assert_eq!(
            members[0].path().file_name(),
            Some(COMPILER_FILE_NAME),
            "a standalone game integration must remain standalone"
        );
    }

    #[test]
    fn dxc_transition_keeps_an_installed_pair_complete() {
        let component = dxc_component(&[COMPILER_FILE_NAME, VALIDATOR_FILE_NAME]);
        let package = dxc_package();

        let members = resolve_transition_members(&component, &package).expect("transition");
        assert_eq!(
            members
                .iter()
                .filter_map(|file| file.path().file_name())
                .collect::<Vec<_>>(),
            [COMPILER_FILE_NAME, VALIDATOR_FILE_NAME],
            "an installed pair must remain a two-file integration"
        );
    }

    #[test]
    fn dxc_transition_rejects_a_validator_without_a_compiler() {
        let component = dxc_component(&[VALIDATOR_FILE_NAME]);

        let error = resolve_transition_members(&component, &dxc_package())
            .expect_err("dxil.dll alone is not a valid DXC integration");
        assert!(error.message().contains(COMPILER_FILE_NAME));
    }

    #[test]
    fn dxc_transition_requires_the_package_to_cover_an_installed_pair() {
        let component = dxc_component(&[COMPILER_FILE_NAME, VALIDATOR_FILE_NAME]);
        let incomplete = LibraryArtifact::new(
            ArtifactId::new("artifact:dxc-incomplete").expect("artifact"),
            GraphicsTechnology::MicrosoftDxc,
            COMPILER_FILE_NAME,
            vec![file(&format!("C:/Library/{COMPILER_FILE_NAME}"), 'a')],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact");

        let error = resolve_transition_members(&component, &incomplete)
            .expect_err("the installed validator must be covered");
        assert!(error.message().contains(VALIDATOR_FILE_NAME));
    }

    #[test]
    fn transition_rejects_a_technology_mismatch() {
        let component = streamline_component(&["sl.common.dll"]);
        let mismatched = LibraryArtifact::new(
            ArtifactId::new("artifact:mismatched-transition").expect("artifact"),
            GraphicsTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![file("C:/Library/nvngx_dlss.dll", 'a')],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact");
        assert!(
            resolve_transition_members(&component, &mismatched)
                .expect_err("technology mismatch")
                .message()
                .contains("technologies do not match")
        );
    }

    #[test]
    fn transition_rejects_duplicate_resolved_targets() {
        let component = streamline_component(&["sl.common.dll"]);
        let duplicate = streamline_artifact(&["sl.common.dll", "SL.COMMON.DLL"]);
        assert!(
            resolve_transition_members(&component, &duplicate)
                .expect_err("duplicate target")
                .message()
                .contains("duplicate install target")
        );
    }
}
