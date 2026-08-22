//! Durable staging of SVAM candidate bytes before the Prepared fence.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use super::super::MutationError;
use super::super::plan::MutationPlan;

pub(crate) fn materialize_stages(plan: &MutationPlan) -> Result<(), MutationError> {
    for directory in &plan.manifest.directories {
        let path = plan.roots.resolve(&directory.path)?;
        fs::create_dir(path).map_err(MutationError::io)?;
    }
    for payload in &plan.payloads {
        let (Some(stage), Some(bytes)) = (&payload.stage_path, &payload.bytes) else {
            continue;
        };
        write_stage(stage, bytes)?;
    }
    Ok(())
}

pub(crate) fn sync_prepared_artifacts(transaction_root: &Path) {
    let snapshots = transaction_root.join("snapshots");
    if snapshots.exists() {
        crate::fs::sync_directory_best_effort(&snapshots);
    }
    crate::fs::sync_directory_best_effort(transaction_root);
    if let Some(parent) = transaction_root.parent() {
        crate::fs::sync_directory_best_effort(parent);
        if let Some(namespace_parent) = parent.parent() {
            crate::fs::sync_directory_best_effort(namespace_parent);
        }
    }
}

fn write_stage(path: &Path, bytes: &[u8]) -> Result<(), MutationError> {
    let parent = path
        .parent()
        .ok_or_else(|| MutationError::conflict("stage path has no parent"))?;
    fs::create_dir_all(parent).map_err(MutationError::io)?;
    match OpenOptions::new().create_new(true).write(true).open(path) {
        Ok(mut file) => {
            file.write_all(bytes).map_err(MutationError::io)?;
            file.sync_all().map_err(MutationError::io)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(MutationError::conflict(
            format!("stage path is occupied: {}", path.display()),
        )),
        Err(error) => Err(MutationError::io(error)),
    }
}
