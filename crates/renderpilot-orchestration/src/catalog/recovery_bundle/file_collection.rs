//! Collection of files referenced by recovery manifests.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use renderpilot_domain::normalized_path_key;
use renderpilot_storage_sqlite::{ConsolidationPlan, SqliteStorage};
use sha2::{Digest, Sha256};

use crate::ServiceError;

use super::manifest::AssociatedFileManifest;
use super::publication::sync_file;

pub(super) fn copy_associated_files(
    storage: &SqliteStorage,
    plan: &ConsolidationPlan,
    temporary: &Path,
) -> Result<(Vec<AssociatedFileManifest>, Vec<String>), ServiceError> {
    let referenced = storage.list_consolidation_recovery_file_paths(plan)?;
    let mut copied = Vec::new();
    let mut missing = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for path in referenced {
        if path.is_file() {
            copy_one_associated_file(&path, temporary, &mut seen, &mut copied)?;
        } else {
            missing.push(path.to_string_lossy().to_string());
        }

        if let Ok(sidecar) = crate::fs::backup_path(&path)
            && sidecar.is_file()
        {
            copy_one_associated_file(&sidecar, temporary, &mut seen, &mut copied)?;
        }
    }

    copied.sort_by(|left, right| left.original_path.cmp(&right.original_path));
    missing.sort();
    missing.dedup();
    Ok((copied, missing))
}

pub(super) fn copy_one_associated_file(
    source: &Path,
    temporary: &Path,
    seen: &mut std::collections::BTreeSet<String>,
    copied: &mut Vec<AssociatedFileManifest>,
) -> Result<(), ServiceError> {
    let original_path = source.to_string_lossy().to_string();
    let normalized = normalized_path_key(&original_path);
    if seen.contains(&normalized) {
        return Ok(());
    }
    let digest = hex::encode(Sha256::digest(normalized.as_bytes()));
    seen.insert(normalized);
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("associated-file");
    let relative_parent = PathBuf::from("associated").join(digest);
    let relative = relative_parent.join(file_name);
    let destination_parent = temporary.join(relative_parent);
    fs::create_dir_all(&destination_parent).map_err(|error| {
        ServiceError::command_failed(format!(
            "could not create associated recovery directory {}: {error}",
            destination_parent.display()
        ))
    })?;
    let destination = temporary.join(&relative);
    fs::copy(source, &destination).map_err(|error| {
        ServiceError::command_failed(format!(
            "could not copy associated recovery file {}: {error}",
            source.display()
        ))
    })?;
    sync_file(&destination)?;
    copied.push(AssociatedFileManifest {
        original_path,
        bundle_path: relative.to_string_lossy().replace('\\', "/"),
    });
    Ok(())
}

pub(super) fn relevant_cover_files(
    storage: &SqliteStorage,
    plan: &ConsolidationPlan,
) -> Result<Vec<String>, ServiceError> {
    let covers = storage.list_all_game_covers()?;
    let mut names = Vec::new();
    if let Some(record) = covers.get(&plan.destination_game_id) {
        names.push(record.file_name.clone());
    }
    for source in &plan.sources {
        if let Some(record) = covers.get(&source.source_game_id) {
            names.push(record.file_name.clone());
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

pub(super) fn sha256_file(path: &Path) -> Result<String, ServiceError> {
    let mut file = fs::File::open(path).map_err(|error| {
        ServiceError::command_failed(format!(
            "could not open recovery file {} for checksum: {error}",
            path.display()
        ))
    })?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            ServiceError::command_failed(format!(
                "could not read recovery file {} for checksum: {error}",
                path.display()
            ))
        })?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(hex::encode(hash.finalize()))
}
