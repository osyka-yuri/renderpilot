//! Read-only, conservative observation of an installation root.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallationPresence {
    /// The requested root is a readable, ordinary directory.
    Present,
    /// A first missing path segment was verified below a readable anchor.
    ConfirmedAbsent {
        /// Evidence bound to this exact root and anchor observation.
        evidence: AbsenceEvidence,
    },
    /// The probe could not distinguish absence from an inaccessible or
    /// inconsistent filesystem state.
    Indeterminate {
        /// Human-readable operation, path, and I/O context.
        reason: String,
    },
}

/// Binds one absence observation to the exact root and anchor that were read.
/// This is observation evidence, not authorization to mutate anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsenceEvidence {
    probed_root: PathBuf,
    readable_anchor: PathBuf,
    missing_segment: PathBuf,
}

impl AbsenceEvidence {
    /// Returns the exact root passed to the probe.
    pub fn probed_root(&self) -> &Path {
        &self.probed_root
    }

    /// Returns the readable volume/share anchor used by the probe.
    pub fn readable_anchor(&self) -> &Path {
        &self.readable_anchor
    }

    /// Returns the first complete missing path prefix below the readable anchor.
    pub fn missing_segment(&self) -> &Path {
        &self.missing_segment
    }
}

/// Probes an absolute root using `symlink_metadata` and readable directory
/// entries. It never writes to the filesystem; callers must re-probe under
/// their lifecycle lock immediately before acting on the result.
pub fn probe_installation_root(root: &Path) -> InstallationPresence {
    let (anchor, segments) = match absolute_path_parts(root) {
        Ok(parts) => parts,
        Err(reason) => return indeterminate(reason),
    };

    let mut parent = match fs::symlink_metadata(&anchor) {
        Ok(metadata) => metadata,
        Err(error) => return io_indeterminate("inspect volume/share anchor", &anchor, &error),
    };
    if let Err(reason) = require_plain_directory(&parent, &anchor) {
        return indeterminate(reason);
    }
    let mut parent_path = anchor.clone();
    let mut ancestors = vec![(anchor.clone(), parent.clone())];
    for segment in segments {
        let current = parent_path.join(&segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if let Err(reason) = require_plain_directory(&metadata, &current) {
                    return indeterminate(reason);
                }
                if let Err(reason) = recheck_directory(&parent_path, &parent) {
                    return indeterminate(reason);
                }
                if let Err(reason) = recheck_directory(&current, &metadata) {
                    return indeterminate(reason);
                }
                parent_path = current;
                parent = metadata;
                ancestors.push((parent_path.clone(), parent.clone()));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match confirm_missing_child(&ancestors, &parent_path, &parent, &current, &segment) {
                    Ok(()) => {
                        return InstallationPresence::ConfirmedAbsent {
                            evidence: AbsenceEvidence {
                                probed_root: root.to_path_buf(),
                                readable_anchor: anchor,
                                missing_segment: current,
                            },
                        };
                    }
                    Err(reason) => return indeterminate(reason),
                }
            }
            Err(error) => {
                return io_indeterminate("inspect installation path", &current, &error);
            }
        }
    }

    if let Err(error) = read_directory(&parent_path) {
        return io_indeterminate("read installation directory", &parent_path, &error);
    }
    if let Err(reason) = recheck_directory(&parent_path, &parent) {
        return indeterminate(reason);
    }
    InstallationPresence::Present
}

fn confirm_missing_child(
    ancestors: &[(PathBuf, fs::Metadata)],
    parent_path: &Path,
    parent_before: &fs::Metadata,
    current: &Path,
    segment: &Path,
) -> Result<(), String> {
    // Present roots need only be readable at the root itself. Absence requires
    // every existing ancestor, through the online volume/share anchor, to be
    // readable before a missing child can be trusted.
    for (path, expected) in ancestors {
        read_directory(path).map_err(|error| {
            format!(
                "cannot read existing ancestor while confirming absence at {}: {error}",
                path.display()
            )
        })?;
        recheck_directory(path, expected)?;
    }

    recheck_directory(parent_path, parent_before)?;

    let entries = fs::read_dir(parent_path).map_err(|error| {
        format!(
            "cannot confirm missing segment {} by reading {}: {error}",
            current.display(),
            parent_path.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "cannot confirm missing segment {} while reading {}: {error}",
                current.display(),
                parent_path.display()
            )
        })?;
        if entry.file_name() == segment.as_os_str() {
            return Err(format!(
                "filesystem observations disagree about installation path {}",
                current.display()
            ));
        }
    }

    let parent_after = fs::symlink_metadata(parent_path).map_err(|error| {
        format!(
            "cannot recheck existing ancestor {}: {error}",
            parent_path.display()
        )
    })?;
    require_plain_directory(&parent_after, parent_path)?;
    if !same_directory_observation(parent_before, &parent_after) {
        return Err(format!(
            "existing ancestor changed during absence probe: {}",
            parent_path.display()
        ));
    }

    match fs::symlink_metadata(current) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(format!(
                "installation path appeared during absence probe: {}",
                current.display()
            ));
        }
        Err(error) => {
            return Err(format!(
                "cannot recheck missing installation segment {}: {error}",
                current.display()
            ));
        }
    }

    recheck_directory(parent_path, &parent_after)
}

fn recheck_directory(path: &Path, before: &fs::Metadata) -> Result<(), String> {
    let after = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot recheck existing ancestor {}: {error}",
            path.display()
        )
    })?;
    require_plain_directory(&after, path)?;
    if !same_directory_observation(before, &after) {
        return Err(format!(
            "existing directory changed during probe: {}",
            path.display()
        ));
    }
    Ok(())
}

fn read_directory(path: &Path) -> io::Result<()> {
    let _ = fs::read_dir(path)?.next().transpose()?;
    Ok(())
}

pub(super) fn absolute_path_parts(root: &Path) -> Result<(PathBuf, Vec<PathBuf>), String> {
    if !root.is_absolute() {
        return Err(format!(
            "installation root must be absolute: {}",
            root.display()
        ));
    }

    let mut anchor = PathBuf::new();
    let mut segments = Vec::new();
    for component in root.components() {
        match component {
            Component::Prefix(prefix) => {
                #[cfg(windows)]
                match prefix.kind() {
                    std::path::Prefix::Disk(_) | std::path::Prefix::UNC(_, _) => {}
                    _ => {
                        return Err(format!(
                            "unsupported or ambiguous installation path prefix: {}",
                            root.display()
                        ));
                    }
                }
                #[cfg(not(windows))]
                let _ = prefix;
                anchor.push(component.as_os_str());
            }
            Component::RootDir => anchor.push(component.as_os_str()),
            Component::Normal(part) => segments.push(PathBuf::from(part)),
            Component::CurDir | Component::ParentDir => {
                return Err(format!(
                    "installation root contains an ambiguous path component: {}",
                    root.display()
                ));
            }
        }
    }
    if anchor.as_os_str().is_empty() {
        return Err(format!(
            "installation root has no volume/share anchor: {}",
            root.display()
        ));
    }
    Ok((anchor, segments))
}

fn require_plain_directory(metadata: &fs::Metadata, path: &Path) -> Result<(), String> {
    if is_link_or_reparse(metadata) {
        return Err(format!(
            "installation path traverses a symbolic link or reparse point: {}",
            path.display()
        ));
    }
    if !metadata.is_dir() {
        return Err(format!(
            "installation path contains a non-directory component: {}",
            path.display()
        ));
    }
    Ok(())
}

fn same_directory_observation(before: &fs::Metadata, after: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        before.dev() == after.dev() && before.ino() == after.ino()
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        before.file_attributes() == after.file_attributes()
            && before.creation_time() == after.creation_time()
    }

    #[cfg(not(any(unix, windows)))]
    {
        before.is_dir() == after.is_dir()
            && before.len() == after.len()
            && before.created().ok() == after.created().ok()
            && before.modified().ok() == after.modified().ok()
    }
}

fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }

    #[cfg(not(windows))]
    false
}

fn io_indeterminate(operation: &str, path: &Path, error: &io::Error) -> InstallationPresence {
    indeterminate(format!("cannot {operation} {}: {error}", path.display()))
}

fn indeterminate(reason: String) -> InstallationPresence {
    InstallationPresence::Indeterminate { reason }
}

#[cfg(test)]
#[path = "presence/tests.rs"]
mod tests;
