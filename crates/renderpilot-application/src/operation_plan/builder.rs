use renderpilot_domain::{ComponentFile, LibraryArtifact, LibraryComponent, PathRef};

use crate::{
    AppError, AppResult, ExternalAliasRequirements, ResolvedPathDisposition, ResolvedTransition,
    resolve_transition,
};

use super::assessment::{OperationPlanAssessment, primary_component_file};
use super::plan::OperationPlanFile;
use super::{OperationPlan, generate_operation_plan_identity};

/// Builds a swap operation plan without applying any filesystem changes.
pub fn build_swap_operation_plan(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> AppResult<OperationPlan> {
    if component.technology() == artifact.technology() {
        let transition = resolve_transition(
            component,
            artifact,
            component.files(),
            &ExternalAliasRequirements::NotRequired,
        )?;
        return build_swap_operation_plan_for_transition(component, artifact, &transition);
    }

    // Keep the legacy inspectable mismatch plan: its single blocker remains a
    // UX diagnostic and never reaches application/execution because the
    // technologies differ. A real transition always comes from the typed
    // resolver below.
    let target_file = primary_component_file(component)?;
    let files = build_mismatch_plan_files(component, artifact)?;
    let assessment = OperationPlanAssessment::assess(component, artifact);
    let identity = generate_operation_plan_identity(component, artifact)?;

    Ok(OperationPlan::new(
        component,
        artifact,
        target_file,
        files,
        assessment,
        identity,
    ))
}

/// Builds preview rows solely from an already-resolved transition.
///
/// Orchestration must call this after its external-alias proof and pass the
/// same contract to preparation, apply, rollback, and journaling.
pub fn build_swap_operation_plan_for_transition(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    transition: &ResolvedTransition,
) -> AppResult<OperationPlan> {
    if transition.component_id() != component.id() || transition.artifact_id() != artifact.id() {
        return Err(AppError::invalid_input(
            "resolved transition does not belong to the supplied component and artifact",
        ));
    }
    let target_file = primary_component_file(component)?;
    let files = build_plan_files(transition);
    let assessment = OperationPlanAssessment::assess(component, artifact);
    let identity = generate_operation_plan_identity(component, artifact)?;

    Ok(OperationPlan::new(
        component,
        artifact,
        target_file,
        files,
        assessment,
        identity,
    ))
}

/// Describes every resolved transition write, keyed by each artifact file's
/// **install target** name (its `install_as`, e.g. the FSR 4 loader →
/// `amd_fidelityfx_dx12.dll`, or its own name). Component-aware package
/// projections such as standalone DXC and Streamline are applied before plan
/// actions are classified.
fn build_plan_files(transition: &ResolvedTransition) -> Vec<OperationPlanFile> {
    transition
        .paths()
        .iter()
        .filter_map(|path| match path {
            ResolvedPathDisposition::Write(write) => Some(match write.current() {
                Some(current) => OperationPlanFile::replace(current, write.source()),
                None => OperationPlanFile::add(write.target().clone(), write.source()),
            }),
            ResolvedPathDisposition::ArchiveAndRemove(archive) => {
                Some(OperationPlanFile::archive_and_remove(
                    archive.target().clone(),
                    archive.current().unwrap_or(archive.baseline()),
                ))
            }
            ResolvedPathDisposition::Remove(remove) => Some(OperationPlanFile::remove(
                remove.target().clone(),
                remove.current(),
            )),
            ResolvedPathDisposition::UntouchedBaseline(_) => None,
        })
        .collect()
}

fn build_mismatch_plan_files(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> AppResult<Vec<OperationPlanFile>> {
    let target_dir = primary_component_file(component)?
        .path()
        .parent()
        .unwrap_or("")
        .to_owned();

    let current_by_name: std::collections::HashMap<String, &ComponentFile> = component
        .files()
        .iter()
        .map(|file| (file_name_key(file.path()), file))
        .collect();

    let transition_members = resolve_plan_members(component, artifact)?;
    let mut files = Vec::with_capacity(transition_members.len());

    for artifact_file in transition_members {
        let install_name = crate::resolve_transition_install_target(component, artifact_file);
        match current_by_name.get(&install_name.to_ascii_lowercase()) {
            Some(current) => files.push(OperationPlanFile::replace(current, artifact_file)),
            None => {
                let target = join_dir_file(&target_dir, &install_name)?;
                files.push(OperationPlanFile::add(target, artifact_file));
            }
        }
    }

    Ok(files)
}

fn resolve_plan_members<'a>(
    component: &LibraryComponent,
    artifact: &'a LibraryArtifact,
) -> AppResult<Vec<&'a ComponentFile>> {
    if component.technology() == artifact.technology() {
        crate::resolve_transition_members(component, artifact)
    } else {
        // Keep the plan inspectable so assessment can expose the explicit
        // TechnologyMismatch blocker instead of failing DTO construction.
        Ok(artifact.files().iter().collect())
    }
}

fn file_name_key(path: &PathRef) -> String {
    path.file_name().unwrap_or("").to_ascii_lowercase()
}

fn join_dir_file(dir: &str, name: &str) -> AppResult<PathRef> {
    let joined = if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    };

    PathRef::new(joined)
        .map_err(|error| AppError::invalid_input(format!("invalid target path: {error}")))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use renderpilot_domain::{
        Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, ComponentFile, ComponentId,
        ComponentKind, GameId, LibraryTechnology, PathRef, PeCompatibilityProfile, PeExportSet,
        PeImportProfile, PeImportSet, RuntimeTarget, Sha256Hash, Swappability,
        xiph::{self, XiphMember},
    };

    use crate::{
        ExternalAliasRequirements, OperationPlanFileAction, ResolvedPathDisposition,
        resolve_transition,
    };

    use super::*;

    fn xiph_file(name: &str, imports: &[&str], hash: char, root: &str) -> ComponentFile {
        let member = xiph::parse_runtime_file_name(name)
            .expect("runtime name")
            .expect("Xiph member")
            .member();
        let export = match member {
            XiphMember::VorbisFile => "ov_open",
            XiphMember::VorbisEnc => "vorbis_encode_init",
            XiphMember::Vorbis => "vorbis_info_init",
            XiphMember::Ogg => "ogg_sync_init",
        };
        ComponentFile::new(PathRef::new(format!("{root}/{name}")).expect("path"))
            .with_sha256(Sha256Hash::new(hash.to_string().repeat(64)).expect("hash"))
            .with_pe_compatibility(
                PeCompatibilityProfile::new(
                    Architecture::X64,
                    PeExportSet::from_observed_names(vec![export.to_owned()]).expect("exports"),
                )
                .with_imports(PeImportProfile {
                    regular: PeImportSet::from_observed_names(
                        imports.iter().map(|name| (*name).to_owned()).collect(),
                    )
                    .expect("imports"),
                    delay: PeImportSet::default(),
                }),
            )
    }

    fn dide_component() -> LibraryComponent {
        [
            xiph_file(
                "vorbisfile_vs2010_x64_rwdi.dll",
                &["vorbis_vs2010_x64_rwdi.dll", "ogg_vs2010_x64_rwdi.dll"],
                '1',
                "C:/Game",
            ),
            xiph_file(
                "vorbis_vs2010_x64_rwdi.dll",
                &["ogg_vs2010_x64_rwdi.dll"],
                '2',
                "C:/Game",
            ),
            xiph_file("ogg_vs2010_x64_rwdi.dll", &[], '3', "C:/Game"),
        ]
        .into_iter()
        .fold(
            LibraryComponent::new(
                ComponentId::new("component:dide-plan").expect("component"),
                GameId::new("game:dide-plan").expect("game"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::XiphVorbis,
                Swappability::BundleOnly,
            ),
            LibraryComponent::with_file,
        )
    }

    fn dide_artifact() -> LibraryArtifact {
        let files = vec![
            xiph_file(
                "vorbisfile.dll",
                &["vorbis.dll", "ogg.dll"],
                'a',
                "C:/Library",
            ),
            xiph_file("vorbis.dll", &["ogg.dll"], 'b', "C:/Library"),
            xiph_file("ogg.dll", &[], 'c', "C:/Library"),
        ];
        LibraryArtifact::new(
            ArtifactId::new("artifact:dide-plan").expect("artifact"),
            LibraryTechnology::XiphVorbis,
            "vorbisfile.dll",
            files,
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
        .with_metadata(
            ArtifactMetadata::default().with_runtime_target(RuntimeTarget::new(Architecture::X64)),
        )
    }

    #[test]
    fn dide_preview_uses_the_resolved_five_live_changing_actions() {
        let component = dide_component();
        let artifact = dide_artifact();
        let transition = resolve_transition(
            &component,
            &artifact,
            component.files(),
            &ExternalAliasRequirements::Proven(BTreeSet::from([
                "vorbisfile_vs2010_x64_rwdi.dll".to_owned()
            ])),
        )
        .expect("transition");
        assert_eq!(
            transition
                .paths()
                .iter()
                .filter(|path| !matches!(path, ResolvedPathDisposition::UntouchedBaseline(_)))
                .count(),
            5
        );

        let plan = build_swap_operation_plan_for_transition(&component, &artifact, &transition)
            .expect("preview");
        let actions = plan
            .files()
            .iter()
            .map(|file| {
                (
                    file.target_path().file_name().expect("target").to_owned(),
                    (file.action(), file.replacement_path().is_some()),
                )
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(actions.len(), 5);
        assert_eq!(
            actions.get("vorbisfile_vs2010_x64_rwdi.dll"),
            Some(&(OperationPlanFileAction::Replace, true))
        );
        assert_eq!(
            actions.get("vorbis.dll"),
            Some(&(OperationPlanFileAction::Add, true))
        );
        assert_eq!(
            actions.get("ogg.dll"),
            Some(&(OperationPlanFileAction::Add, true))
        );
        assert_eq!(
            actions.get("vorbis_vs2010_x64_rwdi.dll"),
            Some(&(OperationPlanFileAction::ArchiveAndRemove, false))
        );
        assert_eq!(
            actions.get("ogg_vs2010_x64_rwdi.dll"),
            Some(&(OperationPlanFileAction::ArchiveAndRemove, false))
        );
        assert_eq!(
            OperationPlanFileAction::ArchiveAndRemove.as_str(),
            "archive_and_remove"
        );
        assert_eq!(OperationPlanFileAction::Remove.as_str(), "remove");
    }
}
