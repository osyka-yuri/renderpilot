//! Durable, closed-world manifest for a shared Vulkan mutation.

use std::collections::BTreeSet;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use super::CapabilityPath;

pub(crate) const MANIFEST_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Scope {
    SharedOnly,
    GameShared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub(crate) version: u8,
    pub(crate) scope: Scope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) game_id: Option<String>,
    pub(crate) feature: String,
    pub(crate) files: Vec<FileParticipant>,
    pub(crate) registry: Vec<RegistryParticipant>,
    pub(crate) directories: Vec<DirectoryParticipant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileParticipant {
    pub(crate) live_path: CapabilityPath,
    pub(crate) before: FileBefore,
    pub(crate) after: FileAfter,
    pub(crate) stage_path: Option<CapabilityPath>,
    pub(crate) tomb_path: Option<CapabilityPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum FileBefore {
    Absent,
    Snapshot {
        snapshot_path: String,
        sha256: String,
        len: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum FileAfter {
    Absent,
    Present { sha256: String, len: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RegistryParticipant {
    pub(crate) manifest_path: CapabilityPath,
    pub(crate) before: RegistryValue,
    pub(crate) after: RegistryValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DirectoryParticipant {
    pub(crate) path: CapabilityPath,
    pub(crate) allowed_direct_children: Vec<CapabilityPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum RegistryValue {
    Absent,
    Present { value_type: u32, raw_bytes: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManifestError(pub(crate) String);

impl std::fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ManifestError {}

impl Manifest {
    pub(crate) fn empty(scope: Scope, game_id: Option<String>, feature: impl Into<String>) -> Self {
        Self {
            version: MANIFEST_VERSION,
            scope,
            game_id,
            feature: feature.into(),
            files: Vec::new(),
            registry: Vec::new(),
            directories: Vec::new(),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ManifestError> {
        if self.version != MANIFEST_VERSION {
            return Err(ManifestError(format!(
                "unsupported shared Vulkan manifest version {}",
                self.version
            )));
        }
        if self.feature.trim().is_empty() {
            return Err(ManifestError("manifest feature is empty".to_owned()));
        }
        match (self.scope, self.game_id.as_deref()) {
            (Scope::SharedOnly, None) => {}
            (Scope::GameShared, Some(game_id)) if !game_id.trim().is_empty() => {}
            (Scope::SharedOnly, Some(_)) => {
                return Err(ManifestError(
                    "shared-only manifest cannot have a game owner".to_owned(),
                ));
            }
            (Scope::GameShared, None) => {
                return Err(ManifestError(
                    "game-shared manifest requires a game owner".to_owned(),
                ));
            }
            _ => return Err(ManifestError("manifest game owner is empty".to_owned())),
        }

        let mut live_paths = BTreeSet::new();
        let mut auxiliary_paths = BTreeSet::new();
        for file in &self.files {
            validate_capability(&file.live_path, "live path")?;
            let live_key = file.live_path.normalized_key();
            if !live_paths.insert(live_key.clone()) {
                return Err(ManifestError(format!("duplicate live target `{live_key}`")));
            }
            match &file.before {
                FileBefore::Absent => {}
                FileBefore::Snapshot {
                    snapshot_path,
                    sha256,
                    len,
                } => {
                    relative_snapshot(snapshot_path)?;
                    validate_digest_and_length(sha256, *len)?;
                }
            }
            match &file.after {
                FileAfter::Absent if file.tomb_path.is_none() => {
                    return Err(ManifestError(format!(
                        "removed file `{live_key}` is missing its tomb path"
                    )));
                }
                FileAfter::Present { sha256, len } => {
                    validate_digest_and_length(sha256, *len)?;
                    if file.stage_path.is_none() {
                        return Err(ManifestError(format!(
                            "written file `{live_key}` is missing its stage path"
                        )));
                    }
                }
                FileAfter::Absent => {}
            }
            if file.after.is_present() != file.stage_path.is_some() {
                return Err(ManifestError(format!(
                    "file `{live_key}` has an inconsistent stage path"
                )));
            }
            if matches!(file.after, FileAfter::Absent) != file.tomb_path.is_some() {
                return Err(ManifestError(format!(
                    "file `{live_key}` has an inconsistent tomb path"
                )));
            }
            for (label, path) in [
                ("stage path", file.stage_path.as_ref()),
                ("tomb path", file.tomb_path.as_ref()),
            ] {
                if let Some(path) = path {
                    validate_capability(path, label)?;
                    if path.root_id() != file.live_path.root_id()
                        || parent_key(path) != parent_key(&file.live_path)
                    {
                        return Err(ManifestError(format!(
                            "{label} must share the live file's capability parent"
                        )));
                    }
                    if !auxiliary_paths.insert(path.normalized_key()) {
                        return Err(ManifestError(format!(
                            "duplicate {label} `{}`",
                            path.normalized_key()
                        )));
                    }
                }
            }
        }
        if let Some(overlap) = overlapping_target(&live_paths) {
            return Err(ManifestError(format!(
                "overlapping live targets `{overlap}`"
            )));
        }
        if auxiliary_paths.iter().any(|auxiliary| {
            live_paths.iter().any(|live| {
                live == auxiliary
                    || live.starts_with(&format!("{auxiliary}/"))
                    || auxiliary.starts_with(&format!("{live}/"))
            })
        }) {
            return Err(ManifestError(
                "stage/tomb path overlaps a live target".to_owned(),
            ));
        }

        let mut registry_paths = BTreeSet::new();
        for registry in &self.registry {
            validate_capability(&registry.manifest_path, "registry manifest path")?;
            if !registry_paths.insert(registry.manifest_path.normalized_key()) {
                return Err(ManifestError("duplicate registry participant".to_owned()));
            }
        }

        let declared_children = live_paths
            .iter()
            .chain(auxiliary_paths.iter())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut directories = BTreeSet::new();
        for directory in &self.directories {
            validate_capability(&directory.path, "directory participant")?;
            let directory_key = directory.path.normalized_key();
            if !directories.insert(directory_key.clone()) {
                return Err(ManifestError(format!(
                    "duplicate directory participant `{directory_key}`"
                )));
            }
            let mut children = BTreeSet::new();
            for child in &directory.allowed_direct_children {
                validate_capability(child, "directory child")?;
                if child.root_id() != directory.path.root_id() || parent_key(child) != directory_key
                {
                    return Err(ManifestError(
                        "directory participant contains a non-direct child".to_owned(),
                    ));
                }
                let child_key = child.normalized_key();
                if !children.insert(child_key.clone()) {
                    return Err(ManifestError(
                        "directory participant contains a duplicate child".to_owned(),
                    ));
                }
                if !declared_children.contains(&child_key)
                    && !self
                        .directories
                        .iter()
                        .any(|candidate| candidate.path.normalized_key() == child_key)
                {
                    return Err(ManifestError(
                        "directory participant authorizes an undeclared child".to_owned(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_for_transaction(&self, mutation_id: &str) -> Result<(), ManifestError> {
        self.validate()?;
        validate_mutation_id(mutation_id)?;
        for (index, file) in self.files.iter().enumerate() {
            if let FileBefore::Snapshot { snapshot_path, .. } = &file.before {
                let expected = format!("snapshots/file-{index}.bin");
                if snapshot_path.replace('\\', "/") != expected {
                    return Err(ManifestError(format!(
                        "snapshot path is not owned by participant {index}"
                    )));
                }
            }
            for (kind, path) in [
                ("stage", file.stage_path.as_ref()),
                ("tomb", file.tomb_path.as_ref()),
            ] {
                let Some(path) = path else { continue };
                let expected = format!(".renderpilot-svam-{mutation_id}-{index}.{kind}");
                let actual = Path::new(path.relative())
                    .file_name()
                    .and_then(|name| name.to_str());
                if actual != Some(expected.as_str()) {
                    return Err(ManifestError(format!(
                        "{kind} path is not owned by transaction `{mutation_id}`"
                    )));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn to_json(&self) -> Result<String, ManifestError> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| ManifestError(format!("could not serialize manifest: {error}")))
    }

    pub(crate) fn from_json(json: &str) -> Result<Self, ManifestError> {
        let manifest: Self = serde_json::from_str(json)
            .map_err(|error| ManifestError(format!("could not parse manifest: {error}")))?;
        manifest.validate()?;
        Ok(manifest)
    }
}

impl FileAfter {
    fn is_present(&self) -> bool {
        matches!(self, Self::Present { .. })
    }
}

fn validate_mutation_id(id: &str) -> Result<(), ManifestError> {
    let path = Path::new(id);
    let mut components = path.components();
    if id.is_empty()
        || path.is_absolute()
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ManifestError(
            "shared Vulkan transaction id is not a single path component".to_owned(),
        ));
    }
    Ok(())
}

fn validate_capability(path: &CapabilityPath, label: &str) -> Result<(), ManifestError> {
    path.validate_shape()
        .map_err(|error| ManifestError(format!("invalid {label}: {error}")))?;
    if path.relative().is_empty() && label != "directory participant" {
        return Err(ManifestError(format!("invalid {label}: path is empty")));
    }
    Ok(())
}

fn relative_snapshot(value: &str) -> Result<(), ManifestError> {
    if value.trim().is_empty() {
        return Err(ManifestError("snapshot path is empty".to_owned()));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || value.as_bytes().get(1) == Some(&b':')
        || value.starts_with("\\\\")
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ManifestError(
            "snapshot path escapes the transaction root".to_owned(),
        ));
    }
    Ok(())
}

fn parent_key(path: &CapabilityPath) -> String {
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

fn overlapping_target(paths: &BTreeSet<String>) -> Option<String> {
    for (index, path) in paths.iter().enumerate() {
        for candidate in paths.iter().skip(index + 1) {
            if candidate.starts_with(&format!("{path}/")) {
                return Some(candidate.clone());
            }
        }
    }
    None
}

fn validate_digest_and_length(digest: &str, _len: u64) -> Result<(), ManifestError> {
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ManifestError(format!("invalid SHA-256 digest `{digest}`")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(relative: &str) -> CapabilityPath {
        CapabilityPath::from_parts("shared", relative).expect("capability")
    }

    fn file(after: FileAfter) -> FileParticipant {
        let present = matches!(&after, FileAfter::Present { .. });
        FileParticipant {
            live_path: cap("ReShade64.dll"),
            before: FileBefore::Absent,
            after,
            stage_path: present.then(|| cap(".renderpilot-svam-test-0.stage")),
            tomb_path: (!present).then(|| cap(".renderpilot-svam-test-0.tomb")),
        }
    }

    #[test]
    fn manifest_rejects_unknown_fields() {
        let error = Manifest::from_json(
            r#"{"version":1,"scope":"shared_only","feature":"x","files":[],"registry":[],"directories":[],"unknown":true}"#,
        )
        .expect_err("closed manifest");
        assert!(error.0.contains("unknown"));
    }

    #[test]
    fn manifest_rejects_scope_owner_mismatch_and_missing_auxiliary_paths() {
        let mut manifest = Manifest::empty(Scope::SharedOnly, Some("game:x".to_owned()), "x");
        assert!(manifest.validate().is_err());
        manifest.game_id = None;
        manifest.files.push(FileParticipant {
            stage_path: None,
            tomb_path: None,
            ..file(FileAfter::Present {
                sha256: "0".repeat(64),
                len: 1,
            })
        });
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn manifest_rejects_duplicate_and_escaping_capability_paths() {
        let mut manifest = Manifest::empty(Scope::SharedOnly, None, "x");
        manifest.files.push(file(FileAfter::Present {
            sha256: "0".repeat(64),
            len: 1,
        }));
        assert!(manifest.validate().is_ok());
        manifest.files.push(file(FileAfter::Present {
            sha256: "1".repeat(64),
            len: 1,
        }));
        assert!(manifest.validate().is_err());
        assert!(CapabilityPath::from_parts("shared", "../escape").is_err());
    }

    #[test]
    fn overlap_detection_is_not_fooled_by_an_intervening_sibling() {
        let paths = BTreeSet::from([
            "shared:a".to_owned(),
            "shared:a-variant".to_owned(),
            "shared:a/child".to_owned(),
        ]);

        assert_eq!(
            overlapping_target(&paths).as_deref(),
            Some("shared:a/child")
        );
    }
}
