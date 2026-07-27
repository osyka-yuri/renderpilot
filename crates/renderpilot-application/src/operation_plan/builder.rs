use std::collections::HashMap;

use renderpilot_domain::{ComponentFile, GraphicsComponent, LibraryArtifact, PathRef, fsr};

use crate::{AppError, AppResult};

use super::assessment::{OperationPlanAssessment, primary_component_file};
use super::plan::OperationPlanFile;
use super::{OperationPlan, generate_operation_plan_identity};

/// Builds a swap operation plan without applying any filesystem changes.
pub fn build_swap_operation_plan(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> AppResult<OperationPlan> {
    let target_file = primary_component_file(component)?;
    let files = build_plan_files(component, artifact)?;
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

    let transition_members = resolve_plan_members(component, artifact)?;
    let mut files = Vec::with_capacity(transition_members.len());

    for artifact_file in transition_members {
        let install_name = fsr::resolve_artifact_install_target(artifact_file, component.files());
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
    component: &GraphicsComponent,
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
