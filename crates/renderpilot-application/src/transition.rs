//! Pure component-aware selection of artifact members installed by a swap.

use std::collections::HashSet;

use renderpilot_domain::{
    ComponentFile, LibraryArtifact, LibraryComponent, LibraryTechnology, fsr, xiph,
};

use crate::{
    AppError, AppResult,
    dxc::{COMPILER_FILE_NAME, VALIDATOR_FILE_NAME},
};

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
/// Xiph packages are selected semantically but must preserve the exact
/// case-insensitive ABI alias already loaded by the game. All other
/// technologies retain the existing install-target policy.
#[must_use]
pub fn resolve_transition_install_target(
    component: &LibraryComponent,
    artifact_file: &ComponentFile,
) -> String {
    if component.technology() == LibraryTechnology::XiphVorbis
        && let Some(artifact_name) = artifact_file
            .install_as()
            .or_else(|| artifact_file.path().file_name())
        && let Some((artifact_member, _)) = xiph::classify_file_name(artifact_name)
        && let Some(installed_name) = component.files().iter().find_map(|file| {
            let name = file.path().file_name()?;
            (xiph::classify_file_name(name).map(|value| value.0) == Some(artifact_member))
                .then_some(name)
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

fn project_xiph_members<'a>(
    component: &LibraryComponent,
    artifact: &'a LibraryArtifact,
) -> AppResult<Vec<&'a ComponentFile>> {
    let mut installed_members = HashSet::new();
    for file in component.files() {
        let name = file
            .path()
            .file_name()
            .ok_or_else(|| AppError::invalid_input("Xiph target has no file name"))?;
        let member = xiph::classify_file_name(name)
            .map(|value| value.0)
            .ok_or_else(|| AppError::invalid_input("Xiph target has an unsupported DLL alias"))?;
        if !installed_members.insert(member) {
            return Err(AppError::invalid_input(
                "Xiph component has duplicate semantic members",
            ));
        }
    }

    let mut package_members = HashSet::new();
    let mut projected = Vec::with_capacity(installed_members.len());
    for file in artifact.files() {
        let name = file
            .install_as()
            .or_else(|| file.path().file_name())
            .ok_or_else(|| AppError::invalid_input("Xiph artifact member has no file name"))?;
        let member = xiph::classify_file_name(name)
            .map(|value| value.0)
            .ok_or_else(|| AppError::invalid_input("Xiph artifact has an unsupported DLL alias"))?;
        if !package_members.insert(member) {
            return Err(AppError::invalid_input(
                "Xiph artifact has duplicate semantic members",
            ));
        }
        if installed_members.contains(&member) {
            projected.push(file);
        }
    }

    let mut missing = installed_members
        .difference(&package_members)
        .map(|member| member.as_slug())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        missing.sort_unstable();
        return Err(AppError::invalid_input(format!(
            "Xiph package does not cover installed members: {}",
            missing.join(", ")
        )));
    }
    Ok(projected)
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

    fn streamline_component(names: &[&str]) -> LibraryComponent {
        names.iter().fold(
            LibraryComponent::new(
                ComponentId::new("component:streamline-transition").expect("component"),
                GameId::new("game:streamline-transition").expect("game"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::NvidiaStreamline,
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
            LibraryTechnology::NvidiaStreamline,
            names[0],
            files,
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
    }

    fn dxc_component(names: &[&str]) -> LibraryComponent {
        names.iter().fold(
            LibraryComponent::new(
                ComponentId::new("component:dxc-transition").expect("component"),
                GameId::new("game:dxc-transition").expect("game"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::MicrosoftDxc,
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
            LibraryTechnology::MicrosoftDxc,
            COMPILER_FILE_NAME,
            vec![
                file(&format!("C:/Library/{COMPILER_FILE_NAME}"), 'a'),
                file(&format!("C:/Library/{VALIDATOR_FILE_NAME}"), 'b'),
            ],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
    }

    fn xiph_component(names: &[&str]) -> LibraryComponent {
        names.iter().fold(
            LibraryComponent::new(
                ComponentId::new("component:xiph-transition").expect("component"),
                GameId::new("game:xiph-transition").expect("game"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::XiphVorbis,
                Swappability::BundleOnly,
            ),
            |component, name| component.with_file(file(&format!("C:/Game/{name}"), 'f')),
        )
    }

    fn xiph_artifact(names: &[&str]) -> LibraryArtifact {
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
            ArtifactId::new("artifact:xiph-transition").expect("artifact"),
            LibraryTechnology::XiphVorbis,
            names[0],
            files,
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
    }

    #[test]
    fn xiph_transition_writes_only_members_present_in_the_game() {
        let component = xiph_component(&["libvorbisfile.dll", "libvorbis.dll", "libogg.dll"]);
        let package = xiph_artifact(&[
            "libvorbis.dll",
            "libvorbisfile.dll",
            "libvorbisenc.dll",
            "libogg.dll",
        ]);

        let members = resolve_transition_members(&component, &package).expect("transition");
        let targets = members
            .iter()
            .map(|member| resolve_transition_install_target(&component, member))
            .collect::<Vec<_>>();

        assert_eq!(
            targets,
            ["libvorbis.dll", "libvorbisfile.dll", "libogg.dll"],
            "the optional encoder must not expand a three-member game integration"
        );
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
            LibraryTechnology::MicrosoftDxc,
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
            LibraryTechnology::DlssSuperResolution,
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
