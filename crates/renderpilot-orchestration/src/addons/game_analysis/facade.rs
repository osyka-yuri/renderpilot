//! High-level Unreal Engine detection facade and presence proofs.

use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};

use renderpilot_detection::pe::{
    MAX_UE3_FILE_VERSION, MIN_UE3_FILE_VERSION, parse_iostore_toc_summary,
    parse_ue3_package_summary,
};

use crate::addons::game_analysis::budget::AnalysisBudget;
use crate::addons::game_analysis::context::GameInstallationContext;
use crate::addons::game_analysis::evidence::{EvidenceScope, EvidenceSource, ValidatedEvidence};
use crate::addons::game_analysis::evidence_set::GameEvidenceSet;
use crate::addons::game_analysis::forensics::{
    DetectionDiagnostic, DetectionReport, ForensicReport, map_codeview_status_to_diagnostics,
    map_coverage_to_diagnostics, resolve_unreal_version,
};
use crate::addons::game_analysis::parsers::metadata_files::{
    parse_build_version, parse_project_descriptor, parse_target_version,
};
use crate::addons::game_analysis::parsers::pdb::{scan_helper_codeview, scan_primary_codeview};
use crate::addons::game_analysis::parsers::release_markers::{
    MarkerScanIncomplete, scan_installation_release_markers,
};
use crate::addons::game_analysis::topology::executable::{
    BoundPrimaryExecutable, TargetPlatformDetection, TopologyError, discover_engine_helpers,
    open_shared_file,
};
use crate::addons::game_analysis::topology::metadata::{
    BoundEngineMetadata, BoundTargetMetadata, ProjectMetadataBindError,
    find_and_bind_project_metadata,
};
use crate::game_executable::ResolvedExecutable;

/// Maximum number of UE3 UPK/PCK/U candidate packages to probe.
pub const MAX_UE3_PROBE_CANDIDATES: usize = 10;

/// Maximum number of IoStore UTOC candidate containers to probe.
pub const MAX_IOSTORE_PROBE_CANDIDATES: usize = 10;

/// External engine detection verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineDetection {
    /// Game reliably identified as using Unreal Engine.
    Unreal(UnrealDetection),
    /// Game engine could not be identified with sufficient confidence.
    UnknownEngine { reason: UnknownReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnknownReason {
    NoExecutablesFound,
    UnreadableExecutable,
    NoUeSignaturesFound,
}

/// Degree of precision of Unreal Engine detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnrealDetection {
    /// Exact version proven down to patch level (e.g. 5.4.3).
    Exact { major: u32, minor: u32, patch: u32 },
    /// Generation and minor proven (e.g. 4.26).
    MajorMinor { major: u32, minor: u32 },
    /// Only generation proven (e.g. UE 5, minor conflicted or absent).
    Generation { major: u32 },
    /// Conflicting major candidates (e.g. UE4 and UE5 signatures simultaneously).
    AmbiguousMajor { candidates: Vec<u32> },
    /// Unreal Engine presence proven, but version cannot be resolved.
    Unknown,
}

impl UnrealDetection {
    #[must_use]
    pub const fn known_major(&self) -> Option<u32> {
        match *self {
            Self::Exact { major, .. }
            | Self::MajorMinor { major, .. }
            | Self::Generation { major } => Some(major),
            Self::AmbiguousMajor { .. } | Self::Unknown => None,
        }
    }
}

/// Positive structural proof of Unreal Engine belonging.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnrealPresenceProof {
    /// Valid .uproject descriptor found.
    ProjectDescriptor { path: PathBuf },
    /// Canonical `<game_root>/Engine/Build/Build.version` parsed.
    CanonicalBuildVersion { path: PathBuf },
    /// Valid strictly bound Primary `<target>.version` parsed.
    TargetVersion { path: PathBuf },
    /// Valid canonical release marker found (`++UE4+Release-` / `++UE5+Release-`).
    ReleaseMarker { source_file: PathBuf, offset: u64 },
    /// Valid canonical PDB path found (`UE_4.x` / `UE_5.x`).
    CanonicalPdb {
        source_file: PathBuf,
        pdb_name: String,
    },
    /// Structurally verified UE3 UPK/PCK/U package summary.
    Ue3Package {
        source_file: PathBuf,
        file_version: u16,
    },
    /// Structurally verified IoStore UTOC container summary.
    IoStoreContainer {
        source_file: PathBuf,
        toc_version: u8,
    },
}

/// Fast UE3 topological probe (Section 12.3).
///
/// Searches `<game_root>/*/CookedPCConsole/`, `<game_root>/*/CookedPC/`, `<game_root>/Engine/CookedPC/`
/// for `.upk`, `.pck`, and `.u` files, normalized and sorted lexicographically, probing the first 10 candidates.
pub fn probe_ue3_installation(
    context: &GameInstallationContext,
) -> Option<(UnrealPresenceProof, UnrealDetection)> {
    let root = context.root_path();

    // Collect search directories
    let mut search_dirs = Vec::new();
    let engine_cooked = root.join("Engine").join("CookedPC");
    if engine_cooked.is_dir() {
        search_dirs.push(engine_cooked);
    }

    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let console_dir = p.join("CookedPCConsole");
                if console_dir.is_dir() {
                    search_dirs.push(console_dir);
                }
                let pc_dir = p.join("CookedPC");
                if pc_dir.is_dir() {
                    search_dirs.push(pc_dir);
                }
            }
        }
    }

    search_dirs.sort();
    search_dirs.dedup();

    let mut heap: BinaryHeap<(String, PathBuf)> = BinaryHeap::new();

    for dir in search_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let is_pkg_ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| {
                        ext.eq_ignore_ascii_case("upk")
                            || ext.eq_ignore_ascii_case("pck")
                            || ext.eq_ignore_ascii_case("u")
                    });
                if !is_pkg_ext {
                    continue;
                }

                // Canonicalize and strictly verify containment within installation root (§12.3)
                let Ok(canonical) = std::fs::canonicalize(&path) else {
                    continue;
                };
                if !canonical.starts_with(context.root_path()) {
                    continue;
                }
                let Ok(relative) = canonical.strip_prefix(context.root_path()) else {
                    continue;
                };
                let Some(relative_str_raw) = relative.to_str() else {
                    continue;
                };
                let relative_str = relative_str_raw.replace('\\', "/");
                if heap.iter().any(|(rel, _)| rel == &relative_str) {
                    continue;
                }
                if heap.len() < MAX_UE3_PROBE_CANDIDATES {
                    heap.push((relative_str, canonical));
                } else if heap.peek().is_some_and(|top| relative_str < top.0) {
                    heap.pop();
                    heap.push((relative_str, canonical));
                }
            }
        }
    }

    let mut candidate_paths = heap.into_sorted_vec();
    candidate_paths.dedup_by(|a, b| a.0 == b.0);

    for (_rel, path) in candidate_paths {
        let Ok(mut file) = open_shared_file(&path) else {
            continue;
        };
        let Ok(meta) = file.metadata() else {
            continue;
        };
        let file_len = meta.len();
        let Ok(summary) = parse_ue3_package_summary(&mut file, file_len) else {
            continue;
        };
        if (MIN_UE3_FILE_VERSION..=MAX_UE3_FILE_VERSION).contains(&summary.file_version) {
            let proof = UnrealPresenceProof::Ue3Package {
                source_file: path,
                file_version: summary.file_version,
            };
            let detection = UnrealDetection::Generation { major: 3 };
            return Some((proof, detection));
        }
    }

    None
}

/// Maps a verified IoStore TOC version to an Unreal Engine major generation according
/// to stock engine mappings (retoc/src/version.rs).
///
/// Returns `Some(4)` for versions 2, 3.
/// Returns `Some(5)` for versions 5, 6, 8.
/// Returns `None` for versions 1, 4, 7, or unknown/unsupported versions.
pub fn generation_from_iostore_version(toc_version: u8) -> Option<u32> {
    match toc_version {
        2 | 3 => Some(4),
        5 | 6 | 8 => Some(5),
        _ => None,
    }
}

/// Helper checking whether `path` or any of its ancestors between `path` and `root`
/// is a symbolic link or reparse point (e.g. NTFS directory junction).
///
/// Threat model & semantics:
/// - Threat model: static filesystem inspection. Assumes no concurrent local adversary
///   actively racing the filesystem between inspection and file opening (true handle-based
///   pinning like `FILE_FLAG_OPEN_REPARSE_POINT` is not used here).
/// - Protects against static symlink traversal, directory junction aliases, or mod packages
///   pointing outside the game tree.
/// - The `root` directory itself is trusted as the installation baseline established
///   by `GameInstallationContext` (allowing games installed on junctioned drive paths).
/// - Any directory junction or symlink *inside* `root` is rejected.
/// - Strictly fail-closed: if querying metadata fails for any component (e.g. permission
///   error, broken reparse point, or I/O error), it returns `true` (rejects candidate),
///   preventing unverified components from passing.
fn has_symlink_component(root: &Path, path: &Path) -> bool {
    let mut current = path;
    while current != root {
        match std::fs::symlink_metadata(current) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return true;
                }
            }
            Err(_) => {
                // Strictly fail-closed: if an ancestor component's metadata cannot be inspected,
                // reject the candidate rather than assuming it is safe.
                return true;
            }
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent,
            _ => break,
        }
    }
    false
}

/// Fast IoStore container topological probe.
///
/// Searches `<game_root>/*/Content/Paks/`, `<game_root>/Content/Paks/`, `<game_root>/Engine/Content/Paks/`
/// for `.utoc` files, normalized and sorted lexicographically, probing the first 10 candidates.
pub fn probe_iostore_installation(
    context: &GameInstallationContext,
) -> Option<(UnrealPresenceProof, Option<u32>)> {
    let root = context.root_path();

    let mut search_dirs = Vec::new();
    let engine_paks = root.join("Engine").join("Content").join("Paks");
    if engine_paks.is_dir() {
        search_dirs.push(engine_paks);
    }
    let root_paks = root.join("Content").join("Paks");
    if root_paks.is_dir() {
        search_dirs.push(root_paks);
    }

    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let paks = p.join("Content").join("Paks");
                if paks.is_dir() {
                    search_dirs.push(paks);
                }
            }
        }
    }

    search_dirs.sort();
    search_dirs.dedup();

    let mut heap: BinaryHeap<(String, PathBuf)> = BinaryHeap::new();

    for dir in search_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if has_symlink_component(root, &path) {
                    continue;
                }
                let is_utoc_ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("utoc"));
                if !is_utoc_ext {
                    continue;
                }

                let Ok(canonical) = std::fs::canonicalize(&path) else {
                    continue;
                };
                if !canonical.starts_with(context.root_path()) {
                    continue;
                }
                let Ok(relative) = canonical.strip_prefix(context.root_path()) else {
                    continue;
                };
                let Some(relative_str_raw) = relative.to_str() else {
                    continue;
                };
                let relative_str = relative_str_raw.replace('\\', "/");
                if heap.iter().any(|(rel, _)| rel == &relative_str) {
                    continue;
                }
                if heap.len() < MAX_IOSTORE_PROBE_CANDIDATES {
                    heap.push((relative_str, canonical));
                } else if heap.peek().is_some_and(|top| relative_str < top.0) {
                    heap.pop();
                    heap.push((relative_str, canonical));
                }
            }
        }
    }

    let mut candidate_paths = heap.into_sorted_vec();
    candidate_paths.dedup_by(|a, b| a.0 == b.0);

    let mut first_valid_proof: Option<UnrealPresenceProof> = None;
    let mut first_ue4_proof: Option<UnrealPresenceProof> = None;
    let mut first_ue5_proof: Option<UnrealPresenceProof> = None;

    for (_rel, path) in candidate_paths {
        let Ok(mut file) = open_shared_file(&path) else {
            continue;
        };
        let Ok(meta) = file.metadata() else {
            continue;
        };
        let file_len = meta.len();
        let Ok(summary) = parse_iostore_toc_summary(&mut file, file_len) else {
            continue;
        };

        let proof = UnrealPresenceProof::IoStoreContainer {
            source_file: path,
            toc_version: summary.toc_version,
        };

        if first_valid_proof.is_none() {
            first_valid_proof = Some(proof.clone());
        }

        match generation_from_iostore_version(summary.toc_version) {
            Some(4) => {
                first_ue4_proof.get_or_insert(proof);
            }
            Some(5) => {
                first_ue5_proof.get_or_insert(proof);
            }
            _ => {}
        }
    }

    let first_valid = first_valid_proof?;

    // Reconcile generations across all valid candidates:
    // - only mapped UE4 versions (2/3) => Generation 4
    // - only mapped UE5 versions (5/6/8) => Generation 5
    // - unmapped versions + mapped UE4 => Generation 4
    // - unmapped versions + mapped UE5 => Generation 5
    // - mapped UE4 + mapped UE5 => generation unresolved / fail closed
    // - only unmapped versions => Unreal presence proof preserved, but generation None
    match (first_ue4_proof, first_ue5_proof) {
        (Some(_), Some(_)) => Some((first_valid, None)),
        (None, Some(p)) => Some((p, Some(5))),
        (Some(p), None) => Some((p, Some(4))),
        (None, None) => Some((first_valid, None)),
    }
}

/// Analyzes an installed game for Unreal Engine presence, versions, and platform architecture.
pub fn analyze_unreal_installation<'game>(
    context: &'game GameInstallationContext,
    resolved_primary: Option<&ResolvedExecutable>,
    budget: &mut AnalysisBudget,
) -> DetectionReport {
    let mut presence_proofs = Vec::new();
    let mut diagnostics = Vec::new();
    let mut scan_coverage = Vec::new();

    // 1. UE3 Fast Path Probe
    if let Some((proof, detection)) = probe_ue3_installation(context) {
        presence_proofs.push(proof);
        return DetectionReport {
            engine: EngineDetection::Unreal(detection),
            platform: TargetPlatformDetection::Unknown,
            presence_proofs,
            forensics: ForensicReport {
                records: Vec::new(),
                major_status: crate::addons::game_analysis::resolver::ResolvedComponent::Determined(
                    3,
                ),
                minor_status: crate::addons::game_analysis::resolver::ResolvedComponent::Unresolved,
                patch_status: crate::addons::game_analysis::resolver::ResolvedComponent::Unresolved,
            },
            diagnostics,
            scan_coverage,
        };
    }

    // 2. Bound Primary Executable verification
    let mut bound_primary: Option<BoundPrimaryExecutable<'game>> = None;
    let mut platform = TargetPlatformDetection::Unknown;

    if let Some(resolved) = resolved_primary {
        match BoundPrimaryExecutable::from_resolved(context, resolved) {
            Ok(primary) => {
                platform = primary.target_platform();
                bound_primary = Some(primary);
            }
            Err(TopologyError::UnsupportedArchitecture { machine, is_64bit }) => {
                platform = TargetPlatformDetection::Unsupported { machine, is_64bit };
                diagnostics.push(DetectionDiagnostic::UnsupportedPlatformArchitecture {
                    machine,
                    is_64bit,
                });
            }
            Err(TopologyError::MalformedPe(reason)) => {
                diagnostics.push(DetectionDiagnostic::MalformedPeHeader {
                    path: Path::new(resolved.path.as_str()).to_path_buf(),
                    reason,
                });
            }
            Err(e) => {
                diagnostics.push(DetectionDiagnostic::MetadataReadFailure {
                    path: Path::new(resolved.path.as_str()).to_path_buf(),
                    reason: format!("{e:?}"),
                });
            }
        }
    } else {
        diagnostics.push(DetectionDiagnostic::MetadataReadFailure {
            path: context.root_path().to_path_buf(),
            reason: "No primary executable found".to_string(),
        });
    }

    let mut evidence_set = GameEvidenceSet::new(context);

    // 3. Build.version probe
    let canonical_build_version_path = context
        .root_path()
        .join("Engine")
        .join("Build")
        .join("Build.version");
    if canonical_build_version_path.is_file() {
        match BoundEngineMetadata::open(context, &canonical_build_version_path) {
            Ok(mut bound_meta) => match parse_build_version(&mut bound_meta) {
                Ok(Some(token)) => {
                    presence_proofs.push(UnrealPresenceProof::CanonicalBuildVersion {
                        path: canonical_build_version_path,
                    });
                    let ev = ValidatedEvidence::from_build_version(token);
                    evidence_set
                        .insert(ev)
                        .expect("guaranteed matching installation context");
                }
                Ok(None) => {}
                Err(e) => {
                    diagnostics.push(DetectionDiagnostic::MetadataReadFailure {
                        path: canonical_build_version_path,
                        reason: format!("{e:?}"),
                    });
                }
            },
            Err(e) => {
                diagnostics.push(DetectionDiagnostic::MetadataReadFailure {
                    path: canonical_build_version_path,
                    reason: format!("{e:?}"),
                });
            }
        }
    }

    // 4. Bound Target <target>.version probe (strictly bound to primary executable)
    if let Some(ref primary) = bound_primary {
        match BoundTargetMetadata::from_primary(primary) {
            Ok(Some(mut bound_target)) => match parse_target_version(&mut bound_target) {
                Ok(Some(token)) => {
                    presence_proofs.push(UnrealPresenceProof::TargetVersion {
                        path: bound_target.path().to_path_buf(),
                    });
                    let ev = ValidatedEvidence::from_target_version(token);
                    evidence_set
                        .insert(ev)
                        .expect("guaranteed matching installation context");
                }
                Ok(None) => {}
                Err(e) => {
                    diagnostics.push(DetectionDiagnostic::MetadataReadFailure {
                        path: bound_target.path().to_path_buf(),
                        reason: format!("{e:?}"),
                    });
                }
            },
            Ok(None) => {}
            Err(e) => {
                let candidate_path = primary.path().with_extension("version");
                diagnostics.push(DetectionDiagnostic::MetadataReadFailure {
                    path: candidate_path,
                    reason: format!("{e:?}"),
                });
            }
        }
    }

    // 5. .uproject descriptor probe
    if let Some(ref primary) = bound_primary {
        match find_and_bind_project_metadata(context, primary.path()) {
            Ok(Some(mut bound_proj)) => match parse_project_descriptor(&mut bound_proj) {
                Ok(outcome) => {
                    if outcome.is_valid_descriptor {
                        presence_proofs.push(UnrealPresenceProof::ProjectDescriptor {
                            path: bound_proj.path().to_path_buf(),
                        });
                    }
                    if let Some(token) = outcome.token {
                        let ev = ValidatedEvidence::from_project_descriptor(token);
                        evidence_set
                            .insert(ev)
                            .expect("guaranteed matching installation context");
                    }
                }
                Err(e) => {
                    diagnostics.push(DetectionDiagnostic::MetadataReadFailure {
                        path: bound_proj.path().to_path_buf(),
                        reason: format!("{e:?}"),
                    });
                }
            },
            Ok(None) => {}
            Err(ProjectMetadataBindError::Ambiguous(conflicting_descriptors)) => {
                diagnostics.push(DetectionDiagnostic::AmbiguousProjectHierarchy {
                    project_descriptors: conflicting_descriptors,
                });
            }
            Err(ProjectMetadataBindError::Topology(e)) => {
                let reason = format!("{e:?}");
                let err_path = match e {
                    TopologyError::OutsideInstallationContext(p)
                    | TopologyError::EngineBinariesDisallowed(p)
                    | TopologyError::EngineServiceHelperDisallowed(p)
                    | TopologyError::SymlinkDisallowed(p)
                    | TopologyError::InvalidExtension(p) => p,
                    _ => primary.path().to_path_buf(),
                };
                diagnostics.push(DetectionDiagnostic::MetadataReadFailure {
                    path: err_path,
                    reason,
                });
            }
        }
    }

    // 6. Engine Helpers discovery
    let mut bound_helpers = if let Some(ref primary) = bound_primary {
        discover_engine_helpers(
            context,
            primary.architecture(),
            budget.max_helpers,
            Some(primary.path()),
        )
    } else {
        Vec::new()
    };

    // 6. CodeView PDB scanning
    if let Some(ref mut primary) = bound_primary {
        match scan_primary_codeview(primary) {
            Ok(scan_res) => {
                map_codeview_status_to_diagnostics(scan_res.status, &mut diagnostics);
                for token in scan_res.tokens {
                    let source_file = token.file_path().to_path_buf();
                    let (ev, pdb_name) = ValidatedEvidence::from_primary_pdb(token);
                    presence_proofs.push(UnrealPresenceProof::CanonicalPdb {
                        source_file,
                        pdb_name,
                    });
                    evidence_set
                        .insert(ev)
                        .expect("guaranteed matching installation context");
                }
            }
            Err(e) => {
                diagnostics.push(DetectionDiagnostic::MetadataReadFailure {
                    path: primary.path().to_path_buf(),
                    reason: e.to_string(),
                });
            }
        }
    }

    for helper in &mut bound_helpers {
        match scan_helper_codeview(helper) {
            Ok(scan_res) => {
                map_codeview_status_to_diagnostics(scan_res.status, &mut diagnostics);
                for token in scan_res.tokens {
                    let source_file = token.file_path().to_path_buf();
                    let (ev, pdb_name) = ValidatedEvidence::from_helper_pdb(token);
                    presence_proofs.push(UnrealPresenceProof::CanonicalPdb {
                        source_file,
                        pdb_name,
                    });
                    evidence_set
                        .insert(ev)
                        .expect("guaranteed matching installation context");
                }
            }
            Err(e) => {
                diagnostics.push(DetectionDiagnostic::MetadataReadFailure {
                    path: helper.path().to_path_buf(),
                    reason: e.to_string(),
                });
            }
        }
    }

    // 7. Streaming Release Marker Scanning (Atomic All-or-Nothing via RAII)
    let has_presence_proof = !presence_proofs.is_empty();
    match scan_installation_release_markers(
        context,
        bound_primary.as_mut(),
        &mut bound_helpers,
        budget,
        &mut scan_coverage,
        has_presence_proof,
    ) {
        Ok(markers) => {
            for ev in markers {
                if ev.source() == EvidenceSource::CanonicalReleaseMarker
                    && (ev.scope() == EvidenceScope::PrimaryExecutable
                        || ev.scope() == EvidenceScope::EngineHelper)
                {
                    presence_proofs.push(UnrealPresenceProof::ReleaseMarker {
                        source_file: ev.file_path().to_path_buf(),
                        offset: ev.file_offset(),
                    });
                }
                evidence_set
                    .insert(ev)
                    .expect("guaranteed matching installation context");
            }
        }
        Err(MarkerScanIncomplete::BudgetExhausted { .. }) => {
            if let Some(ref primary) = bound_primary {
                diagnostics.push(DetectionDiagnostic::MarkerScanIncompleteBudgetExhausted {
                    file_path: primary.path().to_path_buf(),
                });
            }
        }
        Err(MarkerScanIncomplete::TruncatedFile { .. }) => {
            // Real TruncatedSection diagnostic is generated with actual section name,
            // total raw size, and scanned bytes via `map_coverage_to_diagnostics` below.
        }
        Err(MarkerScanIncomplete::IoError { message, .. }) => {
            diagnostics.push(DetectionDiagnostic::MetadataReadFailure {
                path: context.root_path().to_path_buf(),
                reason: message,
            });
        }
        Err(MarkerScanIncomplete::ContextMismatch) => {}
    }

    map_coverage_to_diagnostics(&scan_coverage, &mut diagnostics);

    // Deduplicate presence proofs
    presence_proofs.sort();
    presence_proofs.dedup();

    // 8. Version resolution and presence proof barrier
    let (mut detection, mut forensics) = resolve_unreal_version(&evidence_set);

    // 9. Late IoStore Fallback / Corroboration
    if let Some((iostore_proof, iostore_gen)) = probe_iostore_installation(context) {
        presence_proofs.push(iostore_proof);
        presence_proofs.sort();
        presence_proofs.dedup();

        // Late fallback: if detection is still Unknown, apply IoStore generation when mapped.
        // If already Exact, MajorMinor, Generation, or AmbiguousMajor: do not overwrite!
        if let (UnrealDetection::Unknown, Some(major)) = (&detection, iostore_gen) {
            detection = UnrealDetection::Generation { major };
            forensics.major_status =
                crate::addons::game_analysis::resolver::ResolvedComponent::Determined(major);
        }
    }

    let engine = if presence_proofs.is_empty() {
        if resolved_primary.is_none() {
            EngineDetection::UnknownEngine {
                reason: UnknownReason::NoExecutablesFound,
            }
        } else if bound_primary.is_none() {
            EngineDetection::UnknownEngine {
                reason: UnknownReason::UnreadableExecutable,
            }
        } else {
            EngineDetection::UnknownEngine {
                reason: UnknownReason::NoUeSignaturesFound,
            }
        }
    } else {
        EngineDetection::Unreal(detection)
    };

    DetectionReport {
        engine,
        platform,
        presence_proofs,
        forensics,
        diagnostics,
        scan_coverage,
    }
}
