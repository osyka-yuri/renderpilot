//! Streamline multi-plugin install policy for swap planning.
//!
//! Streamline plugins are a matched set. A replacement must cover every
//! installed plugin basename; extras in the package are skipped so the game's
//! plugin set never expands. Non-Streamline artifacts pass through unchanged.

use std::collections::HashSet;

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::{
    ComponentFile, GraphicsComponent, GraphicsTechnology, LibraryArtifact, fsr,
};

/// Selects which artifact members to install for `component`.
///
/// - Non-Streamline: all artifact files.
/// - Streamline: only members whose install basename is already installed
///   (intersection — package extras never expand the game's plugin set).
/// - Streamline with **multiple** installed plugins: every installed basename
///   must also appear in the package (coverage), otherwise residual mixed
///   versions would remain after a partial apply.
pub(super) fn installable_artifact_files<'a>(
    artifact: &'a LibraryArtifact,
    component: &GraphicsComponent,
) -> AppResult<Vec<&'a ComponentFile>> {
    if artifact.technology() != GraphicsTechnology::NvidiaStreamline {
        return Ok(artifact.files().iter().collect());
    }

    let installed = installed_plugin_names(component)?;
    let (installable, package_names) =
        intersect_package_with_installed(artifact, component, &installed)?;

    // Multi-plugin sets must be fully covered; single-plugin only needs a hit.
    if component.files().len() > 1 {
        require_full_coverage(&installed, &package_names)?;
    }

    Ok(installable)
}

/// Lowercased basenames of installed Streamline plugins (unique).
fn installed_plugin_names(component: &GraphicsComponent) -> AppResult<HashSet<String>> {
    let mut names = HashSet::with_capacity(component.files().len());
    for file in component.files() {
        let Some(name) = file.path().file_name().map(str::to_ascii_lowercase) else {
            continue;
        };
        if !names.insert(name) {
            return Err(AppError::invalid_input(
                "Streamline component has duplicate installed plugin targets",
            ));
        }
    }
    Ok(names)
}

/// Package members whose install basename is already on the game, plus every
/// package install name (for coverage checks). Rejects duplicate package targets.
fn intersect_package_with_installed<'a>(
    artifact: &'a LibraryArtifact,
    component: &GraphicsComponent,
    installed: &HashSet<String>,
) -> AppResult<(Vec<&'a ComponentFile>, HashSet<String>)> {
    let mut package_names = HashSet::with_capacity(artifact.files().len());
    let mut installable = Vec::new();

    for member in artifact.files() {
        let install_name =
            fsr::resolve_artifact_install_target(member, component.files()).to_ascii_lowercase();
        if package_names.contains(&install_name) {
            return Err(AppError::invalid_input(format!(
                "Streamline package has duplicate install target: {install_name}"
            )));
        }
        if installed.contains(&install_name) {
            installable.push(member);
        }
        package_names.insert(install_name);
    }

    Ok((installable, package_names))
}

fn require_full_coverage(
    installed: &HashSet<String>,
    package_names: &HashSet<String>,
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
        "streamline package does not cover installed plugins: {}",
        missing.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentId, ComponentKind, GameId, PathRef, Sha256Hash,
        Swappability, Version,
    };

    fn file(path: &str, version: &str) -> ComponentFile {
        ComponentFile::new(PathRef::new(path).expect("path"))
            .with_sha256(Sha256Hash::new("a".repeat(64)).expect("sha"))
            .with_version(Version::parse(version).expect("version"))
    }

    fn streamline_component(names: &[&str]) -> GraphicsComponent {
        let mut component = GraphicsComponent::new(
            ComponentId::new("component:sl").expect("id"),
            GameId::new("game:a").expect("id"),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::NvidiaStreamline,
            Swappability::BundleOnly,
        );
        for name in names {
            component = component.with_file(file(&format!("C:/Game/{name}"), "2.4.0"));
        }
        component
    }

    fn package(names: &[&str]) -> LibraryArtifact {
        let files: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let sha = char::from(b'a' + index as u8).to_string().repeat(64);
                ComponentFile::new(PathRef::new(format!("C:/Lib/{name}")).expect("path"))
                    .with_sha256(Sha256Hash::new(sha).expect("sha"))
                    .with_version(Version::parse("2.9.0").expect("version"))
            })
            .collect();
        LibraryArtifact::new(
            ArtifactId::new("artifact:sl-pkg").expect("id"),
            GraphicsTechnology::NvidiaStreamline,
            names[0],
            files,
            ArtifactTrustLevel::ManifestDownloaded,
        )
        .expect("package")
    }

    #[test]
    fn multi_plugin_intersects_and_requires_coverage() {
        let component =
            streamline_component(&["sl.common.dll", "sl.dlss.dll", "sl.interposer.dll"]);
        let full = package(&[
            "sl.common.dll",
            "sl.dlss.dll",
            "sl.interposer.dll",
            "sl.pcl.dll",
        ]);
        let installable = installable_artifact_files(&full, &component).expect("covered");
        assert_eq!(installable.len(), 3);
        assert!(
            installable
                .iter()
                .all(|f| !f.path().as_str().contains("sl.pcl"))
        );

        let incomplete = package(&["sl.common.dll", "sl.interposer.dll"]);
        let err = installable_artifact_files(&incomplete, &component).expect_err("missing dlss");
        assert!(
            err.message().contains("sl.dlss.dll"),
            "expected coverage error mentioning missing plugin, got: {}",
            err.message()
        );
    }

    #[test]
    fn single_plugin_component_allows_matching_single_file_artifact() {
        let component = streamline_component(&["sl.common.dll"]);
        let single = package(&["sl.common.dll"]);
        let installable = installable_artifact_files(&single, &component).expect("ok");
        assert_eq!(installable.len(), 1);
    }

    #[test]
    fn single_plugin_ignores_unrelated_package_members() {
        let component = streamline_component(&["sl.common.dll"]);
        let artifact = package(&["sl.dlss.dll", "sl.reflex.dll"]);
        let installable = installable_artifact_files(&artifact, &component).expect("ok");
        assert!(
            installable.is_empty(),
            "no basename overlap → empty install set (caller reports no installable files)"
        );
    }

    #[test]
    fn multi_plugin_rejects_single_file_artifact() {
        let component = streamline_component(&["sl.common.dll", "sl.interposer.dll"]);
        let single = package(&["sl.common.dll"]);
        let err = installable_artifact_files(&single, &component).expect_err("incomplete");
        assert!(err.message().contains("sl.interposer.dll"));
    }

    #[test]
    fn non_streamline_passes_through() {
        let component = GraphicsComponent::new(
            ComponentId::new("component:dlss").expect("id"),
            GameId::new("game:a").expect("id"),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
        )
        .with_file(file("C:/Game/nvngx_dlss.dll", "3.5.0"));
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:dlss").expect("id"),
            GraphicsTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![file("C:/Lib/nvngx_dlss.dll", "3.7.0")],
            ArtifactTrustLevel::ManifestDownloaded,
        )
        .expect("artifact");
        let installable = installable_artifact_files(&artifact, &component).expect("ok");
        assert_eq!(installable.len(), 1);
    }
}
