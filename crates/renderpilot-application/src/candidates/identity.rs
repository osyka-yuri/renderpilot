use std::collections::HashMap;

use renderpilot_domain::{LibraryArtifact, LibraryComponent, Sha256Hash};

use crate::AppResult;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct IdentityMember {
    install_target: String,
    sha256: Sha256Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct RemovalTarget {
    install_target: String,
}

fn identity_members<'a>(
    members: impl IntoIterator<Item = (String, Option<&'a Sha256Hash>)>,
) -> Option<Vec<IdentityMember>> {
    let mut identity = members
        .into_iter()
        .map(|(target, sha256)| {
            let target = target.trim();
            if target.is_empty() {
                return None;
            }
            Some(IdentityMember {
                install_target: target.to_ascii_lowercase(),
                sha256: sha256?.clone(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    if identity.is_empty() {
        return None;
    }
    identity.sort();
    Some(identity)
}

/// Package content under its declared, context-free install targets.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct IntrinsicPackageIdentity(Vec<IdentityMember>);

impl IntrinsicPackageIdentity {
    pub(super) fn for_artifact(artifact: &LibraryArtifact) -> Option<Self> {
        identity_members(artifact.files().iter().map(|file| {
            (
                file.install_as()
                    .or_else(|| file.path().file_name())
                    .unwrap_or_default()
                    .to_owned(),
                file.sha256(),
            )
        }))
        .map(Self)
    }
}

/// Installed content under targets resolved for one concrete component.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct ResolvedTransitionIdentity {
    writes: Vec<IdentityMember>,
    removals: Vec<RemovalTarget>,
}

impl ResolvedTransitionIdentity {
    pub(super) fn for_replacement(
        component: &LibraryComponent,
        artifact: &LibraryArtifact,
    ) -> AppResult<Option<Self>> {
        let members = crate::resolve_transition_members(component, artifact)?;
        let resolved: Vec<_> = members
            .into_iter()
            .map(|file| {
                (
                    file,
                    crate::resolve_transition_install_target(component, file),
                )
            })
            .collect();
        let writes = identity_members(
            resolved
                .iter()
                .map(|(file, target)| (target.clone(), file.sha256())),
        );
        let Some(writes) = writes else {
            return Ok(None);
        };

        let removals = crate::resolve_transition_removals(
            component.files(),
            artifact,
            resolved.iter().map(|(_, target)| target.as_str()),
        )
        .into_iter()
        .map(|file| {
            file.path()
                .file_name()
                .map(str::trim)
                .filter(|target| !target.is_empty())
                .map(|target| RemovalTarget {
                    install_target: target.to_ascii_lowercase(),
                })
        })
        .collect::<Option<Vec<_>>>();
        let Some(mut removals) = removals else {
            return Ok(None);
        };
        removals.sort();
        if removals.windows(2).any(|pair| pair[0] == pair[1]) {
            return Ok(None);
        }

        Ok(Some(Self { writes, removals }))
    }

    /// Projects the installed component onto this transition's concrete targets.
    ///
    /// Extra component members are deliberately ignored: an FSR package can
    /// replace the upscaling stack while leaving separately owned effect DLLs
    /// untouched. Every transition target must still exist exactly once and
    /// carry a hash for the installed identity to be trustworthy.
    pub(super) fn installed_projection(&self, component: &LibraryComponent) -> Option<Self> {
        let mut installed_by_target = HashMap::with_capacity(component.files().len());
        for file in component.files() {
            let target = file.path().file_name()?.trim().to_ascii_lowercase();
            if target.is_empty() || installed_by_target.insert(target, file.sha256()).is_some() {
                return None;
            }
        }

        if self
            .removals
            .iter()
            .any(|removal| installed_by_target.contains_key(&removal.install_target))
        {
            return None;
        }

        identity_members(self.writes.iter().map(|member| {
            (
                member.install_target.clone(),
                installed_by_target
                    .get(&member.install_target)
                    .copied()
                    .flatten(),
            )
        }))
        .map(|writes| Self {
            writes,
            removals: self.removals.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
        LibraryTechnology, PathRef, Swappability,
    };

    use super::*;

    fn file(path: &str, hash: char) -> ComponentFile {
        ComponentFile::new(PathRef::new(path).expect("path"))
            .with_sha256(Sha256Hash::new(hash.to_string().repeat(64)).expect("hash"))
    }

    fn component(files: Vec<ComponentFile>) -> LibraryComponent {
        let component = LibraryComponent::new(
            ComponentId::new("component:identity").expect("component"),
            GameId::new("game:identity").expect("game"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::AmdFsr,
            Swappability::Swappable,
        );
        files
            .into_iter()
            .fold(component, LibraryComponent::with_file)
    }

    #[test]
    fn fsr_intrinsic_identity_is_stable_while_resolved_target_uses_component_lineage() {
        let loader = file("C:/library/amd_fidelityfx_loader_dx12.dll", 'a')
            .with_install_as("amd_fidelityfx_dx12.dll");
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:fsr-loader").expect("artifact"),
            LibraryTechnology::AmdFsr,
            "amd_fidelityfx_loader_dx12.dll",
            vec![loader],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact");
        let unified = component(vec![file("C:/game/amd_fidelityfx_dx12.dll", 'b')]);
        let native = component(vec![file("C:/game/amd_fidelityfx_loader_dx12.dll", 'b')]);

        assert_eq!(
            IntrinsicPackageIdentity::for_artifact(&artifact),
            IntrinsicPackageIdentity::for_artifact(&artifact)
        );
        assert_ne!(
            ResolvedTransitionIdentity::for_replacement(&unified, &artifact)
                .expect("unified identity"),
            ResolvedTransitionIdentity::for_replacement(&native, &artifact)
                .expect("native identity")
        );
    }

    #[test]
    fn installed_projection_ignores_files_outside_the_resolved_transition() {
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:fsr-upscaler").expect("artifact"),
            LibraryTechnology::AmdFsr,
            "amd_fidelityfx_upscaler_dx12.dll",
            vec![file("C:/library/amd_fidelityfx_upscaler_dx12.dll", 'a')],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact");
        let component = component(vec![
            file("C:/game/amd_fidelityfx_upscaler_dx12.dll", 'a'),
            file("C:/game/amd_fidelityfx_denoiser_dx12.dll", 'b'),
        ]);

        let replacement = ResolvedTransitionIdentity::for_replacement(&component, &artifact)
            .expect("resolved identity")
            .expect("non-empty identity");
        assert_eq!(
            replacement.installed_projection(&component),
            Some(replacement),
            "an untouched sibling must not make an installed payload look different"
        );
    }

    #[test]
    fn installed_projection_requires_unified_fsr_cleanup_to_be_complete() {
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:fsr-unified").expect("artifact"),
            LibraryTechnology::AmdFsr,
            "amd_fidelityfx_dx12.dll",
            vec![file("C:/library/amd_fidelityfx_dx12.dll", 'a')],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact");
        let component = component(vec![
            file("C:/game/amd_fidelityfx_dx12.dll", 'a'),
            file("C:/game/amd_fidelityfx_upscaler_dx12.dll", 'b'),
            file("C:/game/amd_fidelityfx_framegeneration_dx12.dll", 'c'),
        ]);

        let replacement = ResolvedTransitionIdentity::for_replacement(&component, &artifact)
            .expect("resolved identity")
            .expect("non-empty identity");
        assert_eq!(
            replacement.installed_projection(&component),
            None,
            "stale split upscaling members make the cleanup transition observable"
        );
    }
}
