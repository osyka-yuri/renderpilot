//! Deterministic participant planning for SVAM-v1.

use std::path::{Path, PathBuf};

use renderpilot_platform_windows::vulkan_layer::LayerRegistry;

use super::TrustedRoots;
use super::io::{digest_hex, file_state};
use super::manifest::{
    DirectoryParticipant, FileAfter, FileBefore, FileParticipant, Manifest, ManifestError,
    RegistryParticipant, RegistryValue, Scope,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileIntent {
    pub(crate) live_path: PathBuf,
    pub(crate) before: Option<Vec<u8>>,
    pub(crate) after: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegistryIntent {
    pub(crate) manifest_path: PathBuf,
    pub(crate) before: RegistryValue,
    pub(crate) after: RegistryValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FilePayload {
    pub(crate) stage_path: Option<PathBuf>,
    pub(crate) tomb_path: Option<PathBuf>,
    pub(crate) bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MutationPlan {
    pub(crate) manifest: Manifest,
    pub(crate) payloads: Vec<FilePayload>,
    pub(crate) roots: TrustedRoots,
}

pub(crate) struct Request<'a> {
    pub(crate) transaction_root: PathBuf,
    pub(crate) mutation_id: String,
    pub(crate) roots: TrustedRoots,
    pub(crate) scope: Scope,
    pub(crate) game_id: Option<String>,
    pub(crate) feature: String,
    pub(crate) intents: Vec<FileIntent>,
    pub(crate) registry: Vec<RegistryIntent>,
    pub(crate) registry_authority: Option<&'a dyn LayerRegistry>,
    pub(crate) created_dirs: Vec<PathBuf>,
}

impl MutationPlan {
    /// Captures all exact before observations and computes all after digests.
    /// No candidate bytes are written by this function.
    pub(crate) fn build(request: Request<'_>) -> Result<Self, super::MutationError> {
        let Request {
            transaction_root,
            mutation_id,
            roots,
            scope,
            game_id,
            feature,
            intents,
            registry,
            registry_authority,
            created_dirs,
        } = request;
        let mut manifest = Manifest::empty(scope, game_id, feature);
        for directory in &created_dirs {
            match std::fs::symlink_metadata(directory) {
                Ok(_) => {
                    return Err(super::MutationError::conflict(format!(
                        "declared created directory already exists: {}",
                        directory.display()
                    )));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(super::MutationError::io(error)),
            }
        }
        let mut created_dirs = created_dirs;
        created_dirs.sort_by_key(|path| path.components().count());
        let mut payloads = Vec::with_capacity(intents.len());

        for intent in intents {
            let live_path = intent.live_path;
            let observed = file_state(&live_path).map_err(super::MutationError::io)?;
            if observed != intent.before {
                return Err(super::MutationError::conflict(format!(
                    "file participant drifted after planning: {}",
                    live_path.display()
                )));
            }
            if intent.before == intent.after {
                continue;
            }
            let index = manifest.files.len();
            let before = match intent.before {
                None => FileBefore::Absent,
                Some(bytes) => {
                    let snapshot_path = transaction_root
                        .join("snapshots")
                        .join(format!("file-{index}.bin"));
                    super::io::write_snapshot(&snapshot_path, &bytes)
                        .map_err(super::MutationError::io)?;
                    FileBefore::Snapshot {
                        snapshot_path: relative_to_root(&transaction_root, &snapshot_path)?,
                        sha256: digest_hex(&bytes),
                        len: bytes.len() as u64,
                    }
                }
            };
            let stage_path = intent
                .after
                .as_ref()
                .map(|_| same_parent_auxiliary(&live_path, &mutation_id, index, "stage"));
            let tomb_path = intent
                .after
                .is_none()
                .then(|| same_parent_auxiliary(&live_path, &mutation_id, index, "tomb"));
            let after = match intent.after.as_ref() {
                None => FileAfter::Absent,
                Some(bytes) => FileAfter::Present {
                    sha256: digest_hex(bytes),
                    len: bytes.len() as u64,
                },
            };
            manifest.files.push(FileParticipant {
                live_path: roots.authorize(&live_path)?,
                before,
                after,
                stage_path: stage_path
                    .as_ref()
                    .map(|path| roots.authorize(path))
                    .transpose()?,
                tomb_path: tomb_path
                    .as_ref()
                    .map(|path| roots.authorize(path))
                    .transpose()?,
            });
            payloads.push(FilePayload {
                stage_path,
                tomb_path,
                bytes: intent.after,
            });
        }

        for intent in registry {
            let authority = registry_authority.ok_or_else(|| {
                super::MutationError::conflict("registry participant requires a registry authority")
            })?;
            let before = super::io::observe_registry(authority, &intent.manifest_path)?;
            if before != intent.before {
                return Err(super::MutationError::conflict(format!(
                    "registry participant drifted after planning: {}",
                    intent.manifest_path.display()
                )));
            }
            if intent.before == intent.after {
                continue;
            }
            manifest.registry.push(RegistryParticipant {
                manifest_path: roots.authorize(&intent.manifest_path)?,
                before: intent.before,
                after: intent.after,
            });
        }
        let directory_paths = created_dirs
            .into_iter()
            .map(|directory| roots.authorize(&directory))
            .collect::<Result<Vec<_>, _>>()?;
        for path in &directory_paths {
            let allowed_direct_children = manifest
                .files
                .iter()
                .flat_map(|file| {
                    [
                        Some(&file.live_path),
                        file.stage_path.as_ref(),
                        file.tomb_path.as_ref(),
                    ]
                    .into_iter()
                    .flatten()
                })
                .chain(directory_paths.iter())
                .filter(|child| {
                    child.normalized_key() != path.normalized_key()
                        && capability_parent_key(child) == path.normalized_key()
                })
                .cloned()
                .collect();
            manifest.directories.push(DirectoryParticipant {
                path: path.clone(),
                allowed_direct_children,
            });
        }
        manifest
            .validate_for_transaction(&mutation_id)
            .map_err(super::MutationError::manifest)?;
        Ok(Self {
            manifest,
            payloads,
            roots,
        })
    }
}

fn same_parent_auxiliary(live: &Path, mutation_id: &str, index: usize, kind: &str) -> PathBuf {
    live.with_file_name(format!(".renderpilot-svam-{mutation_id}-{index}.{kind}"))
}

fn capability_parent_key(path: &super::CapabilityPath) -> String {
    let parent = Path::new(path.relative()).parent().unwrap_or(Path::new(""));
    format!(
        "{}:{}",
        path.root_id(),
        parent
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase()
    )
}

fn relative_to_root(root: &Path, path: &Path) -> Result<String, super::MutationError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        super::MutationError::manifest(ManifestError(
            "snapshot path escaped the transaction root".to_owned(),
        ))
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}
