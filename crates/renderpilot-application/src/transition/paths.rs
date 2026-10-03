//! Shared write targets and path normalization/materialization helpers.

use std::collections::{BTreeMap, btree_map::Entry};

use renderpilot_domain::{ComponentFile, PathRef, normalized_path_key, xiph::XiphMember};

use crate::{AppError, AppResult};

#[derive(Debug, Clone)]
pub(super) struct TransitionWrite {
    pub(super) target: PathRef,
    pub(super) source: ComponentFile,
    pub(super) member: Option<XiphMember>,
}

pub(super) fn insert_transition_write(
    writes: &mut BTreeMap<String, TransitionWrite>,
    write: TransitionWrite,
) -> AppResult<()> {
    let key = normalized_path_key(write.target.as_str());
    match writes.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(write);
            Ok(())
        }
        Entry::Occupied(entry) => Err(AppError::invalid_input(format!(
            "artifact resolves multiple members to install target: {}",
            entry.key()
        ))),
    }
}

pub(super) fn file_map<'a>(
    files: &'a [ComponentFile],
    target_directory: &str,
    label: &str,
    allow_multiple_directories: bool,
) -> AppResult<BTreeMap<String, &'a ComponentFile>> {
    let target_directory_key = if allow_multiple_directories {
        None
    } else {
        Some(normalized_path_key(target_directory))
    };
    let mut mapped = BTreeMap::new();
    for file in files {
        let name = file
            .path()
            .file_name()
            .ok_or_else(|| AppError::invalid_input(format!("{label} file has no file name")))?;
        if name.trim().is_empty() {
            return Err(AppError::invalid_input(format!(
                "{label} file has an empty file name"
            )));
        }
        let parent = file.path().parent().ok_or_else(|| {
            AppError::invalid_input(format!("{label} file has no parent directory"))
        })?;
        if let Some(target_directory_key) = target_directory_key.as_deref()
            && normalized_path_key(parent) != target_directory_key
        {
            return Err(AppError::invalid_input(format!(
                "{label} files do not share one transition directory"
            )));
        }
        if file.sha256().is_none() {
            return Err(AppError::invalid_input(format!(
                "{label} file is missing a SHA-256 hash"
            )));
        }
        let key = normalized_path_key(file.path().as_str());
        if mapped.insert(key, file).is_some() {
            return Err(AppError::invalid_input(format!(
                "{label} has duplicate normalized file paths"
            )));
        }
    }
    Ok(mapped)
}

pub(super) fn join_target(directory: &str, name: &str) -> AppResult<PathRef> {
    if name.trim().is_empty() || name.contains('/') || name.contains('\\') {
        return Err(AppError::invalid_input(
            "artifact resolves an invalid install target",
        ));
    }
    let path = if directory.is_empty() {
        name.to_owned()
    } else {
        format!("{directory}/{name}")
    };
    PathRef::new(path).map_err(|error| {
        AppError::invalid_input(format!("invalid transition target path: {error}"))
    })
}

pub(super) fn materialize_at(source: &ComponentFile, target: PathRef) -> ComponentFile {
    let mut materialized = ComponentFile::new(target);
    if let Some(version) = source.version() {
        materialized = materialized.with_version(version.clone());
    }
    if let Some(hash) = source.sha256() {
        materialized = materialized.with_sha256(hash.clone());
    }
    if let Some(profile) = source.pe_compatibility() {
        materialized = materialized.with_pe_compatibility(profile.clone());
    }
    materialized
}
