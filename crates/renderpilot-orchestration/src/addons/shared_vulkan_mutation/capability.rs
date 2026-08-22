//! Sealed filesystem capabilities for durable SVAM manifests.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{MutationError, Scope};

const CAPABILITY_VERSION: u8 = 1;
const SHARED_ROOT_ID: &str = "shared";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RootKind {
    Game,
    SharedVulkan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootCapability {
    id: String,
    kind: RootKind,
    canonical_path: String,
}

/// Immutable root authority stored separately from the mutable manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrustedRoots {
    version: u8,
    roots: Vec<RootCapability>,
}

/// A manifest path that is meaningful only relative to a sealed root.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CapabilityPath {
    root: String,
    relative: String,
}

impl TrustedRoots {
    pub(crate) fn shared_only(shared_root: &Path) -> Result<Self, MutationError> {
        Self::build(
            Scope::SharedOnly,
            std::iter::empty::<PathBuf>(),
            shared_root,
        )
    }

    pub(crate) fn game_shared(
        game_scope: &crate::file_mutation::MutationScope,
        shared_root: &Path,
    ) -> Result<Self, MutationError> {
        Self::build(
            Scope::GameShared,
            game_scope.roots().iter().cloned(),
            shared_root,
        )
    }

    /// Seals a game-owned shared mutation that has no reachable game-file
    /// participant. This keeps orphan uninstall metadata and app registration
    /// atomic without inventing a filesystem authority for an offline root.
    pub(crate) fn game_shared_without_game_files(
        shared_root: &Path,
    ) -> Result<Self, MutationError> {
        Self::build(
            Scope::GameShared,
            std::iter::empty::<PathBuf>(),
            shared_root,
        )
    }

    fn build(
        scope: Scope,
        game_roots: impl IntoIterator<Item = PathBuf>,
        shared_root: &Path,
    ) -> Result<Self, MutationError> {
        let shared = canonical_root(shared_root)?;
        let mut games = game_roots
            .into_iter()
            .map(|root| canonical_root(&root))
            .collect::<Result<Vec<_>, _>>()?;
        games.sort_by_key(|path| crate::paths::normalized_key(path));
        games.dedup_by(|left, right| crate::paths::same_path(left, right));
        if scope == Scope::SharedOnly && !games.is_empty() {
            return Err(MutationError::conflict(
                "shared-only root authority cannot contain game roots",
            ));
        }
        for (index, root) in games.iter().enumerate() {
            if overlaps(root, &shared)
                || games
                    .iter()
                    .skip(index + 1)
                    .any(|other| overlaps(root, other))
            {
                return Err(MutationError::conflict(
                    "shared Vulkan root capabilities overlap",
                ));
            }
        }
        let mut roots = games
            .into_iter()
            .enumerate()
            .map(|(index, path)| {
                Ok(RootCapability {
                    id: format!("game-{index}"),
                    kind: RootKind::Game,
                    canonical_path: path_to_string(&path)?,
                })
            })
            .collect::<Result<Vec<_>, MutationError>>()?;
        roots.push(RootCapability {
            id: SHARED_ROOT_ID.to_owned(),
            kind: RootKind::SharedVulkan,
            canonical_path: path_to_string(&shared)?,
        });
        let trusted = Self {
            version: CAPABILITY_VERSION,
            roots,
        };
        trusted.validate(scope)?;
        Ok(trusted)
    }

    pub(crate) fn to_json(&self) -> Result<String, MutationError> {
        serde_json::to_string(self).map_err(|error| {
            MutationError::conflict(format!("could not serialize root capabilities: {error}"))
        })
    }

    pub(crate) fn from_json(
        json: &str,
        scope: Scope,
        expected_shared_root: &Path,
    ) -> Result<Self, MutationError> {
        let trusted: Self = serde_json::from_str(json).map_err(|error| {
            MutationError::conflict(format!("invalid root capabilities: {error}"))
        })?;
        trusted.validate(scope)?;
        let expected_shared = canonical_root(expected_shared_root)?;
        let actual_shared = trusted
            .roots
            .iter()
            .find(|root| root.kind == RootKind::SharedVulkan)
            .ok_or_else(|| {
                MutationError::conflict(
                    "persisted root capabilities are missing the shared Vulkan root",
                )
            })?;
        let actual_shared = canonical_root(Path::new(&actual_shared.canonical_path))?;
        if !crate::paths::same_path(&actual_shared, &expected_shared) {
            return Err(MutationError::conflict(
                "persisted shared Vulkan root does not match the platform authority",
            ));
        }
        // Re-resolve every game root. A moved root is a capability mismatch,
        // never a reason to follow the durable path to a new location.
        for root in trusted
            .roots
            .iter()
            .filter(|root| root.kind == RootKind::Game)
        {
            let resolved = canonical_root(Path::new(&root.canonical_path))?;
            if !crate::paths::same_path(&resolved, Path::new(&root.canonical_path)) {
                return Err(MutationError::conflict(
                    "persisted game root no longer resolves to its reserved authority",
                ));
            }
        }
        Ok(trusted)
    }

    fn validate(&self, scope: Scope) -> Result<(), MutationError> {
        if self.version != CAPABILITY_VERSION {
            return Err(MutationError::conflict(format!(
                "unsupported root capability version {}",
                self.version
            )));
        }
        let mut ids = BTreeSet::new();
        let mut paths = Vec::new();
        let mut shared_count = 0usize;
        let mut game_count = 0usize;
        for root in &self.roots {
            if root.id.trim().is_empty() || !ids.insert(root.id.as_str()) {
                return Err(MutationError::conflict(
                    "root capabilities contain a missing or duplicate id",
                ));
            }
            if root.kind == RootKind::SharedVulkan {
                shared_count += 1;
                if root.id != SHARED_ROOT_ID || self.roots.last() != Some(root) {
                    return Err(MutationError::conflict(
                        "shared Vulkan root has an invalid id or ordering",
                    ));
                }
            } else {
                if root.id != format!("game-{game_count}") {
                    return Err(MutationError::conflict(
                        "game root capabilities have invalid ordering",
                    ));
                }
                game_count += 1;
            }
            paths.push(canonical_root(Path::new(&root.canonical_path))?);
        }
        if shared_count != 1 || (scope == Scope::SharedOnly && game_count != 0) {
            return Err(MutationError::conflict(
                "root capabilities do not match the mutation scope",
            ));
        }
        for (index, path) in paths.iter().enumerate() {
            if paths
                .iter()
                .skip(index + 1)
                .any(|other| overlaps(path, other))
            {
                return Err(MutationError::conflict("root capabilities overlap"));
            }
        }
        Ok(())
    }

    pub(crate) fn authorize(&self, absolute: &Path) -> Result<CapabilityPath, MutationError> {
        let candidate = crate::paths::canonical_candidate(absolute)
            .map_err(|error| MutationError::conflict(error.to_string()))?;
        let mut matched = None;
        for root in &self.roots {
            let root_path = Path::new(&root.canonical_path);
            if crate::paths::is_within(&candidate, root_path) {
                let relative = candidate
                    .strip_prefix(root_path)
                    .map_err(|_| MutationError::conflict("authorized path escaped its root"))?;
                let relative = relative_to_string(relative)?;
                if matched
                    .replace(CapabilityPath {
                        root: root.id.clone(),
                        relative,
                    })
                    .is_some()
                {
                    return Err(MutationError::conflict(
                        "authorized path matches overlapping roots",
                    ));
                }
            }
        }
        matched.ok_or_else(|| {
            MutationError::conflict(format!(
                "path is outside trusted mutation roots: {}",
                absolute.display()
            ))
        })
    }

    pub(crate) fn resolve(&self, path: &CapabilityPath) -> Result<PathBuf, MutationError> {
        validate_relative(&path.relative)?;
        let root = self
            .roots
            .iter()
            .find(|root| root.id == path.root)
            .ok_or_else(|| MutationError::conflict("manifest references an unknown root"))?;
        let root_path = Path::new(&root.canonical_path);
        match std::fs::symlink_metadata(root_path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(MutationError::conflict(format!(
                    "trusted mutation root changed type: {}",
                    root_path.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(MutationError::io(error)),
        }
        let candidate = root_path.join(&path.relative);
        if !crate::paths::is_within(&candidate, root_path) {
            return Err(MutationError::conflict(
                "manifest capability path escaped its root",
            ));
        }
        let mut current = root_path.to_path_buf();
        let components = Path::new(&path.relative).components().collect::<Vec<_>>();
        for (index, component) in components.iter().enumerate() {
            let Component::Normal(component) = component else {
                continue;
            };
            current.push(component);
            match std::fs::symlink_metadata(&current) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() {
                        return Err(MutationError::conflict(format!(
                            "manifest capability crosses a symbolic link: {}",
                            current.display()
                        )));
                    }
                    if index + 1 < components.len() && !metadata.is_dir() {
                        return Err(MutationError::conflict(format!(
                            "manifest capability crosses a non-directory: {}",
                            current.display()
                        )));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(MutationError::io(error)),
            }
        }
        Ok(candidate)
    }
}

impl CapabilityPath {
    #[cfg(test)]
    pub(crate) fn from_parts(
        root: impl Into<String>,
        relative: impl Into<String>,
    ) -> Result<Self, MutationError> {
        let path = Self {
            root: root.into(),
            relative: relative.into(),
        };
        path.validate_shape()?;
        Ok(path)
    }

    pub(crate) fn relative(&self) -> &str {
        &self.relative
    }

    pub(crate) fn root_id(&self) -> &str {
        &self.root
    }

    pub(crate) fn validate_shape(&self) -> Result<(), MutationError> {
        if self.root.trim().is_empty() {
            return Err(MutationError::conflict(
                "manifest capability path has no root id",
            ));
        }
        validate_relative(&self.relative)
    }

    pub(crate) fn normalized_key(&self) -> String {
        format!(
            "{}:{}",
            self.root,
            self.relative.replace('\\', "/").to_ascii_lowercase()
        )
    }
}

fn canonical_root(path: &Path) -> Result<PathBuf, MutationError> {
    let canonical = crate::paths::canonical_candidate(path)
        .map_err(|error| MutationError::conflict(error.to_string()))?;
    match std::fs::symlink_metadata(&canonical) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(MutationError::conflict(format!(
                "mutation root is not a regular directory: {}",
                canonical.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(MutationError::io(error)),
    }
    Ok(canonical)
}

fn path_to_string(path: &Path) -> Result<String, MutationError> {
    path.to_str().map(ToOwned::to_owned).ok_or_else(|| {
        MutationError::conflict(format!(
            "mutation root is not valid Unicode: {}",
            path.display()
        ))
    })
}

fn overlaps(left: &Path, right: &Path) -> bool {
    crate::paths::is_within(left, right) || crate::paths::is_within(right, left)
}

fn relative_to_string(path: &Path) -> Result<String, MutationError> {
    let value = path
        .to_str()
        .ok_or_else(|| MutationError::conflict("manifest path is not valid Unicode"))?
        .replace('\\', "/");
    validate_relative(&value)?;
    Ok(value)
}

fn validate_relative(value: &str) -> Result<(), MutationError> {
    let path = Path::new(value);
    if path.is_absolute()
        || value.contains('\\')
        || value.contains(':')
        || value.contains('\0')
        || value.starts_with("//")
        || path.components().any(|component| {
            matches!(
                component,
                Component::CurDir
                    | Component::ParentDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
        || (!value.is_empty()
            && path
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
                != value)
    {
        return Err(MutationError::conflict(
            "manifest capability path is not a validated relative path",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_capability_rejects_escape_and_root_substitution() {
        let temp = tempfile::tempdir().expect("root");
        let shared = temp.path().join("shared");
        std::fs::create_dir(&shared).expect("shared");
        let roots = TrustedRoots::shared_only(&shared).expect("roots");
        assert!(roots.authorize(&shared.join("ReShade64.dll")).is_ok());
        assert!(roots.authorize(&temp.path().join("outside.dll")).is_err());
        for relative in ["./layer.dll", "nested//layer.dll", "layer.dll:stream"] {
            assert!(CapabilityPath::from_parts("shared", relative).is_err());
        }

        let json = roots.to_json().expect("json");
        let other = temp.path().join("other");
        std::fs::create_dir(&other).expect("other");
        assert!(TrustedRoots::from_json(&json, Scope::SharedOnly, &other).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_rejects_a_root_replaced_by_a_symbolic_link() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("root");
        let shared = temp.path().join("shared");
        let outside = temp.path().join("outside");
        let roots = TrustedRoots::shared_only(&shared).expect("roots");
        let target = roots.authorize(&shared.join("layer.dll")).expect("target");
        std::fs::create_dir(&outside).expect("outside");
        symlink(&outside, &shared).expect("replace root with symlink");

        assert!(roots.resolve(&target).is_err());
    }
}
