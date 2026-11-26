//! Engine-specific installation topology detection.
//!
//! This module deliberately reports filesystem evidence only. It does not know
//! about catalog identities, recommendations, or UI confidence. Higher layers
//! combine these facts with launcher and catalog evidence when deciding an
//! installation boundary.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Engine family whose on-disk topology was recognized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    /// Unreal Engine packaged-game topology.
    Unreal,
}

/// Structural role played by the inspected directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineLayoutRole {
    /// Outermost packaged-game directory containing the shared engine and one
    /// or more project trees.
    DistributionRoot,
    /// Project tree such as `<Project>/Binaries` + `<Project>/Content`.
    ProjectSubtree,
    /// Shared `<Distribution>/Engine` tree.
    SharedEngineSubtree,
    /// Directory nested below a project's `Binaries` tree.
    BinarySubtree,
}

/// Typed engine topology evidence for one inspected directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineLayoutEvidence {
    kind: EngineKind,
    role: EngineLayoutRole,
    inspected_root: PathBuf,
    distribution_root: Option<PathBuf>,
    project_root: Option<PathBuf>,
}

impl EngineLayoutEvidence {
    fn unreal(
        role: EngineLayoutRole,
        inspected_root: &Path,
        distribution_root: Option<PathBuf>,
        project_root: Option<PathBuf>,
    ) -> Self {
        Self {
            kind: EngineKind::Unreal,
            role,
            inspected_root: inspected_root.to_path_buf(),
            distribution_root,
            project_root,
        }
    }

    /// Recognized engine family.
    pub const fn kind(&self) -> EngineKind {
        self.kind
    }

    /// Structural role of the inspected directory.
    pub const fn role(&self) -> EngineLayoutRole {
        self.role
    }

    /// Directory that was classified.
    pub fn inspected_root(&self) -> &Path {
        &self.inspected_root
    }

    /// Proven outer distribution root, when the topology establishes one.
    pub fn distribution_root(&self) -> Option<&Path> {
        self.distribution_root.as_deref()
    }

    /// Project root associated with the evidence, when known.
    pub fn project_root(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }
}

/// Filesystem evidence supplied to an engine layout detector.
pub struct EngineLayoutRequest<'a> {
    /// Candidate directory being classified.
    pub candidate: &'a Path,
    /// Readable, accepted game executables discovered within `candidate`.
    pub accepted_executables: &'a [PathBuf],
}

/// Pluggable engine topology detector.
pub trait EngineLayoutDetector {
    /// Returns structural evidence when the candidate participates in this
    /// detector's engine layout.
    fn analyze(&self, request: &EngineLayoutRequest<'_>) -> Option<EngineLayoutEvidence>;
}

/// Runs all supported engine detectors in stable priority order.
pub fn analyze_engine_layout(request: &EngineLayoutRequest<'_>) -> Vec<EngineLayoutEvidence> {
    const DETECTORS: [&dyn EngineLayoutDetector; 1] = [&UnrealLayoutDetector];

    DETECTORS
        .iter()
        .filter_map(|detector| detector.analyze(request))
        .collect()
}

struct UnrealLayoutDetector;

impl EngineLayoutDetector for UnrealLayoutDetector {
    fn analyze(&self, request: &EngineLayoutRequest<'_>) -> Option<EngineLayoutEvidence> {
        let candidate = request.candidate;

        if let Some(project_root) = unreal_project_root(candidate, request.accepted_executables) {
            let distribution_root = unreal_distribution_parent(&project_root);
            let role = if candidate == project_root {
                EngineLayoutRole::ProjectSubtree
            } else {
                EngineLayoutRole::BinarySubtree
            };
            return Some(EngineLayoutEvidence::unreal(
                role,
                candidate,
                distribution_root,
                Some(project_root),
            ));
        }

        if is_shared_engine_root(candidate) {
            return Some(EngineLayoutEvidence::unreal(
                EngineLayoutRole::SharedEngineSubtree,
                candidate,
                candidate.parent().map(Path::to_path_buf),
                None,
            ));
        }

        let project_roots =
            unreal_project_children(candidate, request.accepted_executables).collect::<Vec<_>>();
        if is_plain_directory(&candidate.join("Engine")) && project_roots.len() == 1 {
            return Some(EngineLayoutEvidence::unreal(
                EngineLayoutRole::DistributionRoot,
                candidate,
                Some(candidate.to_path_buf()),
                project_roots.into_iter().next(),
            ));
        }

        // Packaged distributions such as Satisfactory place the selected
        // renderer under the shared `<Distribution>/Engine/Binaries` tree,
        // while the project itself is an immediate sibling (`FactoryGame`).
        // Bind that project only when the shared-engine executable proof and
        // exactly one structural project child are both present.  This keeps
        // the detector bounded and fail-closed for ambiguous distributions.
        let engine_binaries = candidate.join("Engine/Binaries");
        if is_plain_directory(&engine_binaries)
            && request
                .accepted_executables
                .iter()
                .any(|executable| executable.starts_with(&engine_binaries))
        {
            let mut project_roots = plain_child_directories(candidate)
                .into_iter()
                .filter(|path| is_unreal_project_root(path));
            if let (Some(project_root), None) = (project_roots.next(), project_roots.next()) {
                return Some(EngineLayoutEvidence::unreal(
                    EngineLayoutRole::DistributionRoot,
                    candidate,
                    Some(candidate.to_path_buf()),
                    Some(project_root),
                ));
            }
        }

        None
    }
}

fn unreal_project_children<'a>(
    candidate: &'a Path,
    accepted_executables: &'a [PathBuf],
) -> impl Iterator<Item = PathBuf> + 'a {
    plain_child_directories(candidate)
        .into_iter()
        .filter(|path| is_unreal_project_root(path))
        .filter(|project| {
            let binaries = project.join("Binaries");
            accepted_executables
                .iter()
                .any(|executable| executable.starts_with(&binaries))
        })
}

fn unreal_project_root(candidate: &Path, accepted_executables: &[PathBuf]) -> Option<PathBuf> {
    candidate
        .ancestors()
        .find(|ancestor| {
            if !is_unreal_project_root(ancestor) {
                return false;
            }
            let binaries = ancestor.join("Binaries");
            accepted_executables
                .iter()
                .any(|executable| executable.starts_with(&binaries))
        })
        .map(Path::to_path_buf)
}

fn is_unreal_project_root(candidate: &Path) -> bool {
    candidate
        .file_name()
        .is_some_and(|name| !name.as_encoded_bytes().eq_ignore_ascii_case(b"Engine"))
        && is_plain_directory(&candidate.join("Binaries"))
        && is_plain_directory(&candidate.join("Content"))
}

fn unreal_distribution_parent(project_root: &Path) -> Option<PathBuf> {
    let parent = project_root.parent()?;
    (project_root
        .file_name()
        .is_some_and(|name| !name.to_string_lossy().eq_ignore_ascii_case("Engine"))
        && is_plain_directory(&parent.join("Engine")))
    .then(|| parent.to_path_buf())
}

fn is_shared_engine_root(candidate: &Path) -> bool {
    candidate
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("Engine"))
        && is_plain_directory(&candidate.join("Binaries"))
        && candidate.parent().is_some_and(|parent| {
            plain_child_directories(parent)
                .into_iter()
                .any(|path| path != candidate && is_unreal_project_root(&path))
        })
}

fn plain_child_directories(candidate: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(candidate) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_plain_directory(path))
        .collect()
}

fn is_plain_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_dir() && !is_reparse_point(&metadata))
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }

    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn classifies_unreal_distribution_project_and_binary_subtrees() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join("Jedi Survivor");
        let project = root.join("SwGame");
        let binary = project.join("Binaries/Win64/JediSurvivor.exe");
        fs::create_dir_all(root.join("Engine/Binaries")).expect("engine");
        fs::create_dir_all(project.join("Content")).expect("content");
        fs::create_dir_all(binary.parent().expect("binary parent")).expect("binaries");
        fs::write(&binary, b"PE").expect("binary");

        let executable_paths = vec![binary.clone()];
        let root_evidence = analyze_engine_layout(&EngineLayoutRequest {
            candidate: &root,
            accepted_executables: &executable_paths,
        });
        assert_eq!(root_evidence.len(), 1);
        assert_eq!(root_evidence[0].role(), EngineLayoutRole::DistributionRoot);
        assert_eq!(root_evidence[0].distribution_root(), Some(root.as_path()));
        assert_eq!(root_evidence[0].project_root(), Some(project.as_path()));

        let project_evidence = analyze_engine_layout(&EngineLayoutRequest {
            candidate: &project,
            accepted_executables: &executable_paths,
        });
        assert_eq!(project_evidence[0].role(), EngineLayoutRole::ProjectSubtree);
        assert_eq!(
            project_evidence[0].distribution_root(),
            Some(root.as_path())
        );

        let binary_dir = binary.parent().expect("binary parent");
        let binary_evidence = analyze_engine_layout(&EngineLayoutRequest {
            candidate: binary_dir,
            accepted_executables: &executable_paths,
        });
        assert_eq!(binary_evidence[0].role(), EngineLayoutRole::BinarySubtree);
        assert_eq!(binary_evidence[0].distribution_root(), Some(root.as_path()));
    }

    #[test]
    fn standalone_unreal_project_does_not_invent_a_distribution_parent() {
        let temp = tempdir().expect("temp");
        let project = temp.path().join("StandaloneProject");
        let binary = project.join("Binaries/Win64/Game.exe");
        fs::create_dir_all(project.join("Content")).expect("content");
        fs::create_dir_all(binary.parent().expect("binary parent")).expect("binaries");
        fs::write(&binary, b"PE").expect("binary");

        let evidence = analyze_engine_layout(&EngineLayoutRequest {
            candidate: &project,
            accepted_executables: &[binary],
        });
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].role(), EngineLayoutRole::ProjectSubtree);
        assert_eq!(evidence[0].distribution_root(), None);
    }

    #[test]
    fn shared_engine_tree_is_never_a_distribution_root() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join("PackagedGame");
        let engine = root.join("Engine");
        fs::create_dir_all(engine.join("Binaries")).expect("engine");
        fs::create_dir_all(root.join("Project/Binaries")).expect("project binaries");
        fs::create_dir_all(root.join("Project/Content")).expect("project content");

        let evidence = analyze_engine_layout(&EngineLayoutRequest {
            candidate: &engine,
            accepted_executables: &[],
        });
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].role(), EngineLayoutRole::SharedEngineSubtree);
        assert_eq!(evidence[0].distribution_root(), Some(root.as_path()));
    }

    #[test]
    fn project_identity_requires_the_accepted_executable_inside_binaries() {
        let temp = tempdir().expect("temp");
        let project = temp.path().join("Project");
        fs::create_dir_all(project.join("Binaries/Win64")).expect("binaries");
        fs::create_dir_all(project.join("Content")).expect("content");
        let outside = project.join("Game.exe");
        fs::write(&outside, b"PE").expect("outside");

        let evidence = analyze_engine_layout(&EngineLayoutRequest {
            candidate: &project,
            accepted_executables: &[outside],
        });

        assert!(evidence.is_empty());
    }

    #[test]
    fn packaged_shared_engine_executable_binds_the_unique_project_sibling() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join("Satisfactory");
        let project = root.join("FactoryGame");
        let binary = root.join("Engine/Binaries/Win64/FactoryGameSteam-Win64-Shipping.exe");
        fs::create_dir_all(binary.parent().expect("binary parent")).expect("engine binaries");
        fs::create_dir_all(project.join("Binaries/Win64")).expect("project binaries");
        fs::create_dir_all(project.join("Content")).expect("project content");
        fs::write(&binary, b"PE").expect("binary");

        let evidence = analyze_engine_layout(&EngineLayoutRequest {
            candidate: &root,
            accepted_executables: std::slice::from_ref(&binary),
        });

        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].role(), EngineLayoutRole::DistributionRoot);
        assert_eq!(evidence[0].distribution_root(), Some(root.as_path()));
        assert_eq!(evidence[0].project_root(), Some(project.as_path()));
    }

    #[test]
    fn packaged_shared_engine_proof_rejects_ambiguous_or_outside_executables() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join("PackagedGame");
        let project = root.join("Project");
        let second_project = root.join("SecondProject");
        let shared_binary = root.join("Engine/Binaries/Win64/Game.exe");
        let outside = root.join("Game.exe");
        fs::create_dir_all(shared_binary.parent().expect("engine binaries"))
            .expect("engine binaries");
        for candidate in [&project, &second_project] {
            fs::create_dir_all(candidate.join("Binaries")).expect("project binaries");
            fs::create_dir_all(candidate.join("Content")).expect("project content");
        }
        fs::write(&shared_binary, b"PE").expect("shared binary");
        fs::write(&outside, b"PE").expect("outside binary");

        let ambiguous = analyze_engine_layout(&EngineLayoutRequest {
            candidate: &root,
            accepted_executables: std::slice::from_ref(&shared_binary),
        });
        assert!(ambiguous.is_empty());

        let outside_proof = analyze_engine_layout(&EngineLayoutRequest {
            candidate: &root,
            accepted_executables: std::slice::from_ref(&outside),
        });
        assert!(outside_proof.is_empty());
    }
}
