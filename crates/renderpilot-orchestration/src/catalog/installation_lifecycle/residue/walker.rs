//! Bounded no-follow tree walk and tree digest.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use renderpilot_domain::{Sha256Hash, normalized_path_key};
use sha2::{Digest, Sha256};

use crate::fs::{EntryKind, VerifiedDir};

use super::{LocalCleanupCategory, claims::LocalCleanupClaim, path_string};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TreeEntry {
    Directory {
        native_identity: String,
    },
    File {
        native_identity: String,
        digest: Sha256Hash,
    },
}

#[derive(Debug)]
pub(super) struct TreeSnapshot {
    pub(super) native_root_identity: String,
    pub(super) entries: BTreeMap<PathBuf, TreeEntry>,
}

pub(super) enum ClaimedTreeScan {
    Complete(TreeSnapshot),
    NotOnlyKnownRemnants,
}

pub(super) fn scan_tree_against_claims(
    root: &Path,
    expected_root_identity: &str,
    claims: &BTreeMap<String, LocalCleanupClaim>,
) -> Result<ClaimedTreeScan, String> {
    let root_authority = VerifiedDir::open_absolute_components(root, Some(expected_root_identity))
        .map_err(|error| format!("cannot re-open no-follow root authority: {error}"))?;
    let native_root_identity = root_authority.identity().to_owned();
    if native_root_identity.trim().is_empty() {
        return Err("native installation root identity is unavailable".to_owned());
    }

    let mut entries = BTreeMap::new();
    if !walk_directory(root, root, &root_authority, claims, &mut entries)? {
        return Ok(ClaimedTreeScan::NotOnlyKnownRemnants);
    }
    Ok(ClaimedTreeScan::Complete(TreeSnapshot {
        native_root_identity,
        entries,
    }))
}

fn walk_directory(
    root: &Path,
    directory: &Path,
    root_authority: &VerifiedDir,
    claims: &BTreeMap<String, LocalCleanupClaim>,
    entries: &mut BTreeMap<PathBuf, TreeEntry>,
) -> Result<bool, String> {
    let authority = VerifiedDir::open_absolute_components(directory, None).map_err(|error| {
        format!(
            "cannot open no-follow directory {}: {error}",
            directory.display()
        )
    })?;
    let mut children = Vec::new();
    for item in fs::read_dir(directory).map_err(|error| {
        format!(
            "cannot enumerate installation directory {}: {error}",
            directory.display()
        )
    })? {
        let item = item.map_err(|error| {
            format!(
                "cannot read installation directory entry {}: {error}",
                directory.display()
            )
        })?;
        let name = item.file_name();
        if name.to_str().is_none() {
            return Err(format!(
                "installation tree contains a non-text path below {}",
                directory.display()
            ));
        }
        let path = item.path();
        if !matches!(
            path.strip_prefix(root)
                .ok()
                .and_then(|relative| relative.components().next()),
            Some(std::path::Component::Normal(_))
        ) {
            return Err(format!(
                "installation tree contains an ambiguous path: {}",
                path.display()
            ));
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "cannot inspect installation entry {}: {error}",
                path.display()
            )
        })?;
        children.push((path, metadata));
    }

    // Stop before hashing the first unknown file. In particular, ordinary
    // game archives do not get read during catalog presence checks.
    children.sort_by(|left, right| left.0.cmp(&right.0));
    for (path, metadata) in children {
        if is_link_or_reparse(&metadata) {
            return Err(format!(
                "installation tree contains a link or reparse point: {}",
                path.display()
            ));
        }
        let path_key = normalized_path_key(&path_string(&path));
        let claim = claims.get(&path_key);
        if metadata.is_dir() {
            if claim.is_some_and(|claim| claim.category() != LocalCleanupCategory::Directory) {
                return Ok(false);
            }
            let child_authority =
                VerifiedDir::open_absolute_components(&path, None).map_err(|error| {
                    format!(
                        "cannot open installation directory without following links {}: {error}",
                        path.display()
                    )
                })?;
            let child_identity = child_authority.identity().to_owned();
            if child_identity.trim().is_empty() {
                return Err(format!(
                    "directory identity is unavailable: {}",
                    path.display()
                ));
            }
            if let Some(claim) = claim {
                let Some(expected_identity) = claim.native_identity() else {
                    return Ok(false);
                };
                if expected_identity != child_identity {
                    return Ok(false);
                }
            }
            let relative = relative_text_path(root, &path)?;
            entries.insert(
                relative,
                TreeEntry::Directory {
                    native_identity: child_identity.clone(),
                },
            );
            if !walk_directory(root, &path, root_authority, claims, entries)? {
                return Ok(false);
            }
        } else if metadata.is_file() {
            let Some(claim) = claim else {
                return Ok(false);
            };
            if claim.category() == LocalCleanupCategory::Directory || claim.sha256().is_none() {
                return Ok(false);
            }
            #[cfg(test)]
            record_hashed_claim_path(&path);
            let observation = root_authority
                .observe_descendant(&path)
                .map_err(|error| {
                    format!("cannot observe regular file {}: {error}", path.display())
                })?
                .ok_or_else(|| format!("file disappeared during root proof: {}", path.display()))?;
            if observation.kind != EntryKind::File {
                return Err(format!(
                    "installation entry is not a regular file: {}",
                    path.display()
                ));
            }
            let digest = observation
                .digest
                .ok_or_else(|| format!("file digest is unavailable: {}", path.display()))?;
            let digest = Sha256Hash::new(digest).map_err(|error| {
                format!("file digest is invalid at {}: {error}", path.display())
            })?;
            if claim.sha256() != Some(&digest)
                || claim
                    .native_identity()
                    .is_some_and(|expected| expected != observation.identity)
            {
                return Ok(false);
            }
            entries.insert(
                relative_text_path(root, &path)?,
                TreeEntry::File {
                    native_identity: observation.identity,
                    digest,
                },
            );
        } else {
            return Err(format!(
                "installation tree contains an unsupported entry: {}",
                path.display()
            ));
        }
    }

    VerifiedDir::open_absolute_components(directory, Some(authority.identity())).map_err(
        |error| {
            format!(
                "installation directory changed during root proof {}: {error}",
                directory.display()
            )
        },
    )?;
    Ok(true)
}

fn relative_text_path(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| format!("installation entry escaped its root: {}", path.display()))?;
    if relative.to_str().is_none() {
        return Err(format!(
            "installation entry path is not text: {}",
            path.display()
        ));
    }
    Ok(relative.to_path_buf())
}

pub(super) fn tree_digest(snapshot: &TreeSnapshot) -> Result<Sha256Hash, String> {
    let mut hasher = Sha256::new();
    hasher.update(snapshot.native_root_identity.as_bytes());
    hasher.update([0]);
    for (relative, entry) in &snapshot.entries {
        let relative = relative
            .to_str()
            .ok_or_else(|| "installation tree contains a non-text path".to_owned())?;
        hasher.update(relative.replace('\\', "/").as_bytes());
        hasher.update([0]);
        match entry {
            TreeEntry::Directory { native_identity } => {
                hasher.update(b"directory");
                hasher.update([0]);
                hasher.update(native_identity.as_bytes());
            }
            TreeEntry::File {
                native_identity,
                digest,
            } => {
                hasher.update(b"file");
                hasher.update([0]);
                hasher.update(native_identity.as_bytes());
                hasher.update([0]);
                hasher.update(digest.as_str().as_bytes());
            }
        }
        hasher.update([0]);
    }
    Sha256Hash::new(hex::encode(hasher.finalize()))
        .map_err(|error| format!("could not encode root snapshot digest: {error}"))
}

#[cfg(test)]
thread_local! {
    static CLAIMED_FILE_OBSERVATIONS: std::cell::RefCell<Vec<PathBuf>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

#[cfg(test)]
fn record_hashed_claim_path(path: &Path) {
    CLAIMED_FILE_OBSERVATIONS.with(|paths| paths.borrow_mut().push(path.to_path_buf()));
}

#[cfg(test)]
pub(super) fn reset_claimed_file_observations() {
    CLAIMED_FILE_OBSERVATIONS.with(|paths| paths.borrow_mut().clear());
}

#[cfg(test)]
pub(super) fn claimed_file_observations() -> Vec<PathBuf> {
    CLAIMED_FILE_OBSERVATIONS.with(|paths| paths.borrow().clone())
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
