//! Deterministic private-workspace derivation for the OptiScaler journal.
//!
//! A workspace is shared only by artifact-bearing operations that select the
//! same outermost scope root and existing filesystem.  The filesystem
//! identity remains native-only evidence: the durable journal retains the
//! derived root index, path, capability, and materialized directory identity.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use renderpilot_domain::{OperationEffect as DomainOperationEffect, OptiScalerJournal};

use crate::ServiceError;

#[derive(Debug, Clone)]
pub(super) struct PlannedWorkspace {
    workspace_id: u32,
    root_index: u32,
    path: PathBuf,
}

impl PlannedWorkspace {
    pub(super) const fn workspace_id(&self) -> u32 {
        self.workspace_id
    }

    pub(super) const fn root_index(&self) -> u32 {
        self.root_index
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug)]
struct WorkspaceProgram {
    workspaces: Vec<PlannedWorkspace>,
    operation_workspaces: Vec<Option<u32>>,
}

impl WorkspaceProgram {
    fn workspace_id_for(&self, operation_id: usize) -> Result<Option<u32>, ServiceError> {
        self.operation_workspaces
            .get(operation_id)
            .copied()
            .ok_or_else(|| crate::failed("workspace program is shorter than the operation program"))
    }
}

/// Plans all private workspaces before the journal is persisted.
pub(super) fn planned_workspaces(
    transaction_id: &str,
    capability: &str,
    planned: &[super::OptiScalerPlannedOperation],
    roots: &[PathBuf],
) -> Result<(Vec<PlannedWorkspace>, Vec<Option<u32>>), ServiceError> {
    let program = derive_planned_workspace_program(transaction_id, capability, planned, roots)?;
    Ok((program.workspaces, program.operation_workspaces))
}

/// Re-derives every durable workspace from the complete stored program and
/// verifies all native filesystem boundaries before execution or recovery.
pub(super) fn validate_paths(
    transaction_id: &str,
    journal: &OptiScalerJournal,
) -> Result<(), ServiceError> {
    validated_workspace_program(transaction_id, journal)?;
    Ok(())
}

fn validated_workspace_program(
    transaction_id: &str,
    journal: &OptiScalerJournal,
) -> Result<WorkspaceProgram, ServiceError> {
    let roots = journal
        .roots()
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let barriers = barriers_from_journal(journal)?;
    let program = derive_journal_workspace_program(transaction_id, journal, &roots, &barriers)?;
    if program.workspaces.len() != journal.private_workspaces().len() {
        return Err(crate::failed(
            "private workspace program does not match its durable bindings",
        ));
    }
    for (expected, actual) in program.workspaces.iter().zip(journal.private_workspaces()) {
        if expected.workspace_id() != actual.workspace_id()
            || expected.root_index() != actual.root_index()
            || !crate::paths::same_path(expected.path(), Path::new(actual.path()))
        {
            return Err(crate::failed(
                "private workspace path is not derived from the complete operation closure",
            ));
        }
    }
    for (index, operation) in journal.operations().iter().enumerate() {
        if operation.workspace_id() != program.workspace_id_for(index)? {
            return Err(crate::failed(
                "operation workspace reference is not derived from its physical group",
            ));
        }
    }
    Ok(program)
}

/// Returns the derived workspace path for an identity-bound native operation.
pub(super) fn derived_workspace_path(
    transaction_id: &str,
    journal: &OptiScalerJournal,
    workspace_id: u32,
) -> Result<PathBuf, ServiceError> {
    let program = validated_workspace_program(transaction_id, journal)?;
    let workspace = program
        .workspaces
        .iter()
        .find(|candidate| candidate.workspace_id() == workspace_id)
        .ok_or_else(|| crate::failed("workspace id is outside the journal"))?;
    Ok(workspace.path().to_path_buf())
}

fn derive_planned_workspace_program(
    transaction_id: &str,
    capability: &str,
    planned: &[super::OptiScalerPlannedOperation],
    roots: &[PathBuf],
) -> Result<WorkspaceProgram, ServiceError> {
    let barriers = barriers_from_planned(planned);
    let mut workspaces = Vec::new();
    let mut operation_workspaces = Vec::with_capacity(planned.len());
    let mut physical_groups = HashMap::<(u32, String), u32>::new();

    for operation in planned {
        let Some(endpoint) = planned_artifact_endpoint(operation) else {
            operation_workspaces.push(None);
            continue;
        };
        let (root_index, host, filesystem) = derive_host_for_endpoint(endpoint, roots, &barriers)?;
        let key = (root_index, filesystem);
        let workspace_id = if let Some(workspace_id) = physical_groups.get(&key) {
            *workspace_id
        } else {
            let workspace_id = u32::try_from(workspaces.len())
                .map_err(|_| crate::failed("workspace program is too large"))?;
            let path = host.join(format!(
                ".renderpilot-optiscaler-workspace-{transaction_id}-{workspace_id}-{capability}"
            ));
            workspaces.push(PlannedWorkspace {
                workspace_id,
                root_index,
                path,
            });
            physical_groups.insert(key, workspace_id);
            workspace_id
        };
        operation_workspaces.push(Some(workspace_id));
    }
    Ok(WorkspaceProgram {
        workspaces,
        operation_workspaces,
    })
}

fn derive_journal_workspace_program(
    transaction_id: &str,
    journal: &OptiScalerJournal,
    roots: &[PathBuf],
    barriers: &[PathBuf],
) -> Result<WorkspaceProgram, ServiceError> {
    let capability = journal.control_namespace().capability().as_str();
    let mut workspaces = Vec::new();
    let mut operation_workspaces = Vec::with_capacity(journal.operations().len());
    let mut physical_groups = HashMap::<(u32, String), u32>::new();
    for (index, operation) in journal.operations().iter().enumerate() {
        if !operation.effect().requires_private_workspace() {
            operation_workspaces.push(None);
            continue;
        }
        let endpoint = operation
            .effect()
            .endpoints()
            .into_iter()
            .next()
            .ok_or_else(|| crate::failed("artifact-bearing operation has no endpoint"))?;
        let (root_index, host, filesystem) =
            derive_host_for_endpoint(Path::new(endpoint.path()), roots, barriers)?;
        let key = (root_index, filesystem);
        let workspace_id = if let Some(workspace_id) = physical_groups.get(&key) {
            *workspace_id
        } else {
            let workspace_id = u32::try_from(workspaces.len())
                .map_err(|_| crate::failed("workspace program is too large"))?;
            let path = host.join(format!(
                ".renderpilot-optiscaler-workspace-{transaction_id}-{workspace_id}-{capability}"
            ));
            workspaces.push(PlannedWorkspace {
                workspace_id,
                root_index,
                path,
            });
            physical_groups.insert(key.clone(), workspace_id);
            workspace_id
        };
        for endpoint in operation.effect().endpoints() {
            let endpoint_parent = existing_directory(
                Path::new(endpoint.path())
                    .parent()
                    .ok_or_else(|| crate::failed("operation endpoint has no parent"))?,
            )
            .ok_or_else(|| {
                crate::failed("operation endpoint has no existing directory ancestor")
            })?;
            let endpoint_filesystem =
                crate::fs::VerifiedDir::open(&endpoint_parent)?.filesystem_identity()?;
            if endpoint_filesystem != key.1 {
                return Err(crate::failed(format!(
                    "OptiScaler atomic custody crosses filesystems at ordinal {index}"
                )));
            }
        }
        operation_workspaces.push(Some(workspace_id));
    }
    Ok(WorkspaceProgram {
        workspaces,
        operation_workspaces,
    })
}

fn planned_artifact_endpoint(operation: &super::OptiScalerPlannedOperation) -> Option<&Path> {
    match operation {
        super::OptiScalerPlannedOperation::Write(value)
        | super::OptiScalerPlannedOperation::Delete(value)
        | super::OptiScalerPlannedOperation::CreateDirectory(value) => Some(&value.path),
        super::OptiScalerPlannedOperation::Verify(_)
        | super::OptiScalerPlannedOperation::Relocate { .. }
        | super::OptiScalerPlannedOperation::PostCommitRemoveDirectory(_) => None,
    }
}

fn barriers_from_planned(planned: &[super::OptiScalerPlannedOperation]) -> Vec<PathBuf> {
    planned
        .iter()
        .filter_map(|operation| match operation {
            super::OptiScalerPlannedOperation::CreateDirectory(value) => Some(value.path.clone()),
            super::OptiScalerPlannedOperation::PostCommitRemoveDirectory(receipt) => {
                Some(PathBuf::from(receipt.path.as_str()))
            }
            _ => None,
        })
        .collect()
}

fn barriers_from_journal(journal: &OptiScalerJournal) -> Result<Vec<PathBuf>, ServiceError> {
    journal
        .operations()
        .iter()
        .filter_map(|operation| match operation.effect() {
            DomainOperationEffect::CreateDirectory(effect) => Some(effect.endpoint().path()),
            DomainOperationEffect::PostCommitRemoveDirectory(effect) => {
                Some(effect.endpoint().path())
            }
            _ => None,
        })
        .map(|path| Ok(PathBuf::from(path)))
        .collect()
}

fn derive_host_for_endpoint(
    endpoint: &Path,
    roots: &[PathBuf],
    barriers: &[PathBuf],
) -> Result<(u32, PathBuf, String), ServiceError> {
    let root_index = select_outermost_root(endpoint, roots)?;
    let selected_root = roots
        .get(usize::try_from(root_index).map_err(|_| crate::failed("root index overflow"))?)
        .ok_or_else(|| crate::failed("selected workspace root is missing"))?;
    let mut host = endpoint
        .parent()
        .ok_or_else(|| crate::failed("artifact-bearing operation has no parent"))?
        .to_path_buf();
    while let Some(barrier) = barriers
        .iter()
        .filter(|barrier| {
            crate::paths::same_path(&host, barrier) || crate::paths::is_within(&host, barrier)
        })
        .min_by_key(|barrier| component_depth(barrier))
    {
        host = barrier
            .parent()
            .ok_or_else(|| crate::failed("workspace barrier has no parent"))?
            .to_path_buf();
    }
    if !crate::paths::is_within(&host, selected_root) {
        return Err(crate::failed(
            "derived private workspace host is outside the selected custody root",
        ));
    }
    if barriers.iter().any(|barrier| {
        crate::paths::same_path(&host, barrier) || crate::paths::is_within(&host, barrier)
    }) {
        return Err(crate::failed("derived private workspace host is removable"));
    }
    let directory = crate::fs::VerifiedDir::open(&host).map_err(|error| {
        crate::failed(format!(
            "derived private workspace host is not an existing verified directory: {}: {error}",
            host.display()
        ))
    })?;
    Ok((root_index, host, directory.filesystem_identity()?))
}

fn select_outermost_root(endpoint: &Path, roots: &[PathBuf]) -> Result<u32, ServiceError> {
    roots
        .iter()
        .enumerate()
        .filter(|(_, root)| crate::paths::is_within(endpoint, root))
        .min_by(|(_, left), (_, right)| {
            component_depth(left)
                .cmp(&component_depth(right))
                .then_with(|| {
                    crate::paths::normalized_key(left).cmp(&crate::paths::normalized_key(right))
                })
        })
        .map(|(index, _)| u32::try_from(index).map_err(|_| crate::failed("root index overflow")))
        .transpose()?
        .ok_or_else(|| crate::failed("artifact endpoint is outside the custody roots"))
}

fn component_depth(path: &Path) -> usize {
    path.components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count()
}

fn existing_directory(path: &Path) -> Option<PathBuf> {
    let mut candidate = path;
    loop {
        if crate::fs::VerifiedDir::open(candidate).is_ok() {
            return Some(candidate.to_path_buf());
        }
        let parent = candidate.parent()?;
        if crate::paths::same_path(parent, candidate) {
            return None;
        }
        candidate = parent;
    }
}
