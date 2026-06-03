use std::collections::HashMap;

use renderpilot_domain::{ComponentFile, GraphicsComponent, LibraryArtifact, PathRef};

use crate::{AppError, AppResult};

use super::assessment::{primary_component_file, OperationPlanAssessment};
use super::plan::OperationPlanFile;
use super::{generate_operation_plan_identity, OperationPlan};

/// Builds a swap operation plan without applying any filesystem changes.
pub fn build_swap_operation_plan(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> AppResult<OperationPlan> {
    let target_file = primary_component_file(component)?;
    let files = build_plan_files(component, artifact)?;
    let assessment = OperationPlanAssessment::assess(component, artifact);
    let identity = generate_operation_plan_identity(component, artifact);

    Ok(OperationPlan::new(
        component,
        artifact,
        target_file,
        files,
        assessment,
        identity,
    ))
}

/// Describes every write the additive swap performs, keyed by each artifact
/// file's **install target** name (its `install_as`, e.g. the FSR 4 loader →
/// `amd_fidelityfx_dx12.dll`, or its own name): a target already present in the
/// component is a replace, a new target is an add. The swap never removes the
/// game's other files, so there is no remove entry.
fn build_plan_files(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> AppResult<Vec<OperationPlanFile>> {
    let target_dir = primary_component_file(component)?
        .path()
        .parent()
        .unwrap_or("")
        .to_owned();

    let current_by_name: HashMap<String, &ComponentFile> = component
        .files()
        .iter()
        .map(|file| (file_name_key(file.path()), file))
        .collect();
    let component_file_names: Vec<&str> = component
        .files()
        .iter()
        .filter_map(|file| file.path().file_name())
        .collect();

    let mut files = Vec::with_capacity(artifact.files().len());

    for artifact_file in artifact.files() {
        let default_install_name = artifact_file
            .install_as()
            .or_else(|| artifact_file.path().file_name())
            .unwrap_or("");
        let install_name = crate::fsr::resolve_loader_install_target(
            default_install_name,
            component_file_names.iter().copied(),
        );
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
