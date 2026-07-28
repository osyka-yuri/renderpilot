//! Serializable output DTOs shared between the CLI and GUI API layers.
//!
//! These types convert catalog result structs into JSON-friendly shapes that
//! are stable across releases. Both `renderpilot-cli` and `renderpilot-api`
//! use them so the wire format stays consistent.

use renderpilot_application::{
    ComponentReplacementCandidates, D3d12ExecutableAction, InstalledReleaseState, OperationPlan,
    OperationPlanFile, ReplacementCandidate,
};
use renderpilot_domain::{CatalogPackageAvailability, PackageRelease};
use serde::Serialize;

use super::{
    D3d12ExecutableStatus, OperationListCatalogEntry, OperationListCatalogResult, RollbackPlan,
    execute::OperationMetadata,
};

// -----------------------------------------------------------------------------
// Candidate output types
// -----------------------------------------------------------------------------

/// Serializable shape for one component's replacement candidates.
#[derive(Debug, Serialize)]
pub struct ComponentCandidateOutput {
    /// Stable identifier of the component.
    pub component_id: String,
    /// Technology slug (`"dlss_super_resolution"`, etc.).
    pub technology: String,
    /// Installed file path of the component.
    pub file_path: String,
    /// Honest installed-version state for this component.
    pub version_report: InstalledReleaseStateOutput,
    /// Available replacement candidates for this component.
    pub candidates: Vec<CandidateOutput>,
}

/// JSON-safe representation of the installed component version state.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstalledReleaseStateOutput {
    /// All relevant installed files resolve to one version.
    Known {
        /// Technical PE FileVersion used for compatibility, when present.
        technical_version: Option<String>,
        /// Supplemental catalog release annotation.
        release_label: Option<String>,
        /// Exact catalog release when installed content matches a known package.
        catalog_release: Option<PackageRelease>,
    },
    /// Known installed files prove the component is on multiple releases.
    Mixed {
        /// Lowest known technical PE FileVersion.
        min_technical_version: String,
        /// Highest known technical PE FileVersion.
        max_technical_version: String,
    },
    /// Metadata cannot establish a trustworthy installed version.
    Unknown,
}

impl From<&InstalledReleaseState> for InstalledReleaseStateOutput {
    fn from(report: &InstalledReleaseState) -> Self {
        match report {
            InstalledReleaseState::Known {
                technical_version,
                release_label,
                catalog_release,
            } => Self::Known {
                technical_version: technical_version
                    .as_ref()
                    .map(|version| version.as_str().to_owned()),
                release_label: release_label.clone(),
                catalog_release: catalog_release.clone(),
            },
            InstalledReleaseState::Mixed {
                min_technical_version,
                max_technical_version,
            } => Self::Mixed {
                min_technical_version: min_technical_version.as_str().to_owned(),
                max_technical_version: max_technical_version.as_str().to_owned(),
            },
            InstalledReleaseState::Unknown => Self::Unknown,
        }
    }
}

/// Serializable shape for a single replacement candidate artifact.
#[derive(Debug, Serialize)]
pub struct CandidateOutput {
    /// Stable artifact id.
    pub artifact_id: String,
    /// Artifact file name.
    pub file_name: String,
    /// Absolute path to the locally cached artifact, if downloaded.
    pub file_path: Option<String>,
    /// Technical PE FileVersion, if available.
    pub technical_version: Option<String>,
    /// Supplemental catalog release label.
    pub release_label: Option<String>,
    /// Game id the artifact was extracted from, if any.
    pub source_game_id: Option<String>,
    /// Comparison result against the currently installed version.
    pub comparison: String,
    /// Curated package identity, release, availability, and selection capability.
    pub catalog_package: Option<CatalogCandidateOutput>,
    /// Whether the artifact has been downloaded locally.
    pub is_downloaded: bool,
    /// Whether the artifact is a debug build.
    pub is_debug: bool,
    /// SHA-256 hex digest of the artifact.
    pub sha256: String,
    /// Required action for the main executable, for D3D12 candidates.
    pub d3d12_executable_action: Option<D3d12ExecutableActionOutput>,
}

/// Nested catalog-package metadata attached to a candidate.
#[derive(Debug, Serialize)]
pub struct CatalogCandidateOutput {
    /// Stable logical package identity.
    pub package_id: String,
    /// Exact package release used for presentation and sorting.
    pub release: PackageRelease,
    /// Whether the package remains active or exists only as a local receipt.
    pub availability: CatalogPackageAvailability,
    /// Whether unattended selection is permitted.
    pub automatic_selection_allowed: bool,
}

/// Stable wire shape for D3D12 executable assessment.
#[derive(Debug, Clone, Serialize)]
pub struct D3d12ExecutableActionOutput {
    /// `none`, `patch`, `restore`, or `repair_required`.
    pub kind: String,
    /// Main executable path.
    pub executable_path: String,
    /// Immutable sidecar path.
    pub backup_path: String,
    /// Whether the immutable original sidecar already exists.
    pub backup_exists: bool,
    /// SDK line from the original executable.
    pub original_sdk_version: u32,
    /// SDK line currently active.
    pub current_sdk_version: u32,
    /// SDK line requested by the candidate/rollback.
    pub target_sdk_version: u32,
    /// Whether apply must carry the fresh swap-plan token.
    pub requires_confirmation: bool,
}

impl From<&D3d12ExecutableAction> for D3d12ExecutableActionOutput {
    fn from(action: &D3d12ExecutableAction) -> Self {
        Self {
            kind: action.kind().as_str().to_owned(),
            executable_path: action.executable_path().as_str().to_owned(),
            backup_path: action.backup_path().as_str().to_owned(),
            backup_exists: action.backup_exists(),
            original_sdk_version: action.original_sdk_version(),
            current_sdk_version: action.current_sdk_version(),
            target_sdk_version: action.target_sdk_version(),
            requires_confirmation: action.requires_confirmation(),
        }
    }
}

/// Stable wire shape for current managed D3D12 executable status.
#[derive(Debug, Clone, Serialize)]
pub struct D3d12ExecutableStatusOutput {
    /// `original`, `patched`, or `repair_required`.
    pub status: String,
    /// Whether executable selection is locked to the captured aggregate.
    pub selection_locked: bool,
    /// Main executable path.
    pub executable_path: String,
    /// Expected immutable backup path, whether or not it currently exists.
    pub backup_path: String,
    /// Whether the immutable backup currently exists.
    pub backup_exists: bool,
    /// Original SDK line.
    pub original_sdk_version: u32,
    /// Currently active SDK line.
    pub current_sdk_version: u32,
}

impl From<&D3d12ExecutableStatus> for D3d12ExecutableStatusOutput {
    fn from(status: &D3d12ExecutableStatus) -> Self {
        let state = if status.repair_required() {
            "repair_required"
        } else if status.current_sdk_version() == status.original_sdk_version() {
            "original"
        } else {
            "patched"
        };
        Self {
            status: state.to_owned(),
            selection_locked: status.selection_locked(),
            executable_path: status.executable_path().as_str().to_owned(),
            backup_path: status.backup_path().as_str().to_owned(),
            backup_exists: status.backup_exists(),
            original_sdk_version: status.original_sdk_version(),
            current_sdk_version: status.current_sdk_version(),
        }
    }
}

/// Serializable swap preflight shared by CLI and API.
#[derive(Debug, Clone, Serialize)]
pub struct SwapPlanOutput {
    /// Generated operation id.
    pub operation_id: String,
    /// Fresh state-bound confirmation token.
    pub confirmation_token: String,
    /// Target game id.
    pub game_id: String,
    /// Target component id.
    pub component_id: String,
    /// Stable operation kind.
    pub operation_type: String,
    /// Primary active target path.
    pub target_path: String,
    /// Primary replacement source path.
    pub replacement_path: String,
    /// Installed primary version.
    pub original_version: Option<String>,
    /// Replacement primary version.
    pub replacement_version: Option<String>,
    /// Installed primary hash.
    pub original_sha256: Option<String>,
    /// Replacement primary hash.
    pub replacement_sha256: Option<String>,
    /// Derived risk level.
    pub risk_level: String,
    /// Whether target writes are likely to require elevation.
    pub requires_elevation: bool,
    /// Selected artifact id.
    pub artifact_id: String,
    /// Findings that prevent apply.
    pub blockers: Vec<String>,
    /// Non-blocking findings.
    pub warnings: Vec<String>,
    /// Complete file mutation list.
    pub files: Vec<SwapPlanFileOutput>,
    /// D3D12 executable action, when applicable.
    pub d3d12_executable_action: Option<D3d12ExecutableActionOutput>,
}

/// One file in a bundle swap preflight.
#[derive(Debug, Clone, Serialize)]
pub struct SwapPlanFileOutput {
    /// Stable file action.
    pub action: String,
    /// Active target path.
    pub target_path: String,
    /// Replacement source path, when the action copies a file.
    pub replacement_path: Option<String>,
    /// Installed version.
    pub original_version: Option<String>,
    /// Replacement version.
    pub replacement_version: Option<String>,
    /// Installed hash.
    pub original_sha256: Option<String>,
    /// Replacement hash.
    pub replacement_sha256: Option<String>,
}

impl From<&OperationPlanFile> for SwapPlanFileOutput {
    fn from(file: &OperationPlanFile) -> Self {
        Self {
            action: file.action().as_str().to_owned(),
            target_path: file.target_path().as_str().to_owned(),
            replacement_path: file.replacement_path().map(|path| path.as_str().to_owned()),
            original_version: file
                .original_version()
                .map(|version| version.as_str().to_owned()),
            replacement_version: file
                .replacement_version()
                .map(|version| version.as_str().to_owned()),
            original_sha256: file.original_sha256().map(|hash| hash.as_str().to_owned()),
            replacement_sha256: file
                .replacement_sha256()
                .map(|hash| hash.as_str().to_owned()),
        }
    }
}

impl From<&OperationPlan> for SwapPlanOutput {
    fn from(plan: &OperationPlan) -> Self {
        Self {
            operation_id: plan.operation_id().as_str().to_owned(),
            confirmation_token: plan.confirmation_token().to_owned(),
            game_id: plan.game_id().as_str().to_owned(),
            component_id: plan.component_id().as_str().to_owned(),
            operation_type: plan.operation_type().as_str().to_owned(),
            target_path: plan.target_path().as_str().to_owned(),
            replacement_path: plan.replacement_path().as_str().to_owned(),
            original_version: plan
                .original_version()
                .map(|version| version.as_str().to_owned()),
            replacement_version: plan
                .replacement_version()
                .map(|version| version.as_str().to_owned()),
            original_sha256: plan.original_sha256().map(|hash| hash.as_str().to_owned()),
            replacement_sha256: plan
                .replacement_sha256()
                .map(|hash| hash.as_str().to_owned()),
            risk_level: plan.risk_level().as_str().to_owned(),
            requires_elevation: plan.requires_elevation(),
            artifact_id: plan.artifact_id().as_str().to_owned(),
            blockers: plan
                .blockers()
                .iter()
                .map(|blocker| blocker.as_str().to_owned())
                .collect(),
            warnings: plan
                .warnings()
                .iter()
                .map(|warning| warning.as_str().to_owned())
                .collect(),
            files: plan.files().iter().map(SwapPlanFileOutput::from).collect(),
            d3d12_executable_action: plan
                .d3d12_executable_action()
                .map(D3d12ExecutableActionOutput::from),
        }
    }
}

/// Serializable rollback preflight shared by CLI and API.
#[derive(Debug, Clone, Serialize)]
pub struct RollbackPlanOutput {
    /// Target game id.
    pub game_id: String,
    /// Target component id.
    pub component_id: String,
    /// All live and sidecar paths touched by rollback.
    pub affected_files: Vec<String>,
    /// Managed executable restore, when applicable.
    pub d3d12_executable_action: Option<D3d12ExecutableActionOutput>,
}

impl From<&RollbackPlan> for RollbackPlanOutput {
    fn from(plan: &RollbackPlan) -> Self {
        Self {
            game_id: plan.game_id().as_str().to_owned(),
            component_id: plan.component_id().as_str().to_owned(),
            affected_files: plan
                .affected_files()
                .iter()
                .map(|path| path.as_str().to_owned())
                .collect(),
            d3d12_executable_action: plan
                .d3d12_executable_action()
                .map(D3d12ExecutableActionOutput::from),
        }
    }
}

impl From<ComponentReplacementCandidates> for ComponentCandidateOutput {
    fn from(group: ComponentReplacementCandidates) -> Self {
        let candidates = group
            .candidates()
            .iter()
            .map(CandidateOutput::from)
            .collect();
        Self {
            component_id: group.component_id().as_str().to_owned(),
            technology: group.technology().as_slug().to_owned(),
            file_path: group.file_path().as_str().to_owned(),
            version_report: InstalledReleaseStateOutput::from(group.installed_release()),
            candidates,
        }
    }
}

impl From<&ReplacementCandidate> for CandidateOutput {
    fn from(candidate: &ReplacementCandidate) -> Self {
        Self {
            artifact_id: candidate.artifact_id().as_str().to_owned(),
            file_name: candidate.file_name().to_owned(),
            file_path: candidate.file_path().map(|path| path.as_str().to_owned()),
            technical_version: candidate
                .technical_version()
                .map(|version| version.as_str().to_owned()),
            release_label: candidate.release_label().map(str::to_owned),
            source_game_id: candidate
                .source_game_id()
                .map(|game_id| game_id.as_str().to_owned()),
            comparison: candidate.comparison().as_str().to_owned(),
            catalog_package: candidate
                .catalog_package()
                .map(|package| CatalogCandidateOutput {
                    package_id: package.package_id().to_owned(),
                    release: package.release().clone(),
                    availability: package.availability(),
                    automatic_selection_allowed: package.automatic_selection_allowed(),
                }),
            is_downloaded: candidate.is_downloaded(),
            is_debug: candidate.is_debug(),
            sha256: candidate.sha256().to_owned(),
            d3d12_executable_action: candidate
                .d3d12_executable_action()
                .map(D3d12ExecutableActionOutput::from),
        }
    }
}

/// Converts a slice of [`ComponentReplacementCandidates`] into serializable output DTOs.
pub fn component_candidate_outputs(
    groups: Vec<ComponentReplacementCandidates>,
) -> Vec<ComponentCandidateOutput> {
    groups
        .into_iter()
        .map(ComponentCandidateOutput::from)
        .collect()
}

// -----------------------------------------------------------------------------
// Operation summary output
// -----------------------------------------------------------------------------

/// Serializable summary of a single operation record.
#[derive(Debug, Serialize)]
pub struct OperationSummaryOutput {
    /// Stable operation id.
    pub operation_id: String,
    /// Operation kind string (`"swap"`, `"rollback"`, etc.).
    pub kind: String,
    /// Current status string (`"completed"`, `"running"`, etc.).
    pub status: String,
    /// Unix timestamp (milliseconds) when the operation was created.
    pub created_at: i64,
    /// Unix timestamp (milliseconds) when the operation completed, if finished.
    pub completed_at: Option<i64>,
    /// Number of files affected by the operation.
    pub item_count: usize,
    /// Id of the primary component affected.
    pub component_id: String,
    /// Typed operation metadata, if the stored journal entry uses the current schema.
    pub metadata: Option<OperationMetadata>,
}

impl From<&OperationListCatalogEntry> for OperationSummaryOutput {
    fn from(entry: &OperationListCatalogEntry) -> Self {
        let metadata = entry
            .operation
            .metadata_json
            .as_ref()
            .and_then(|metadata| serde_json::from_str(metadata.as_str()).ok());

        Self {
            operation_id: entry.operation.id.as_str().to_owned(),
            kind: entry.operation.kind.as_str().to_owned(),
            status: entry.operation.status.as_str().to_owned(),
            created_at: entry.operation.created_at.as_i64(),
            completed_at: entry
                .operation
                .completed_at
                .map(|timestamp| timestamp.as_i64()),
            item_count: entry.item_count,
            component_id: entry.component_ids.first().cloned().unwrap_or_default(),
            metadata,
        }
    }
}

/// Converts an [`OperationListCatalogResult`] into a flat list of serializable summaries.
pub fn operation_summary_outputs(
    result: &OperationListCatalogResult,
) -> Vec<OperationSummaryOutput> {
    result
        .operations
        .iter()
        .map(OperationSummaryOutput::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        CandidateOutput, CatalogCandidateOutput, D3d12ExecutableActionOutput,
        D3d12ExecutableStatusOutput, InstalledReleaseStateOutput, OperationSummaryOutput,
        SwapPlanOutput,
    };
    use renderpilot_application::{
        D3d12ExecutableAction, D3d12ExecutableProfile, InstalledReleaseState, MetadataJson,
        OperationKind, OperationPlanBlocker, OperationRecord, OperationStatus, UnixTimestampMillis,
        build_swap_operation_plan,
    };
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, CatalogPackageAvailability, ComponentFile, ComponentId,
        ComponentKind, GameId, GraphicsComponent, GraphicsTechnology, LibraryArtifact, OperationId,
        PackageRelease, PackageVersion, PathRef, ReleaseChannel, Sha256Hash, Swappability, Version,
    };
    use serde_json::json;

    use crate::catalog::{D3d12ExecutableStatus, OperationListCatalogEntry};

    #[test]
    fn developer_mode_blockers_and_recalculated_risk_have_stable_wire_values() {
        let component = GraphicsComponent::new(
            ComponentId::new("component:d3d12-wire").expect("component"),
            GameId::new("game:d3d12-wire").expect("game"),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::D3D12Agility,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(PathRef::new("C:/Game/D3D12Core.dll").expect("component path"))
                .with_sha256(
                    Sha256Hash::new(
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    )
                    .expect("component hash"),
                ),
        );
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:d3d12-wire").expect("artifact id"),
            GraphicsTechnology::D3D12Agility,
            "D3D12Core.dll",
            vec![
                ComponentFile::new(
                    PathRef::new("C:/Library/D3D12Core.dll").expect("artifact path"),
                )
                .with_sha256(
                    Sha256Hash::new(
                        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    )
                    .expect("artifact hash"),
                ),
            ],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact");
        let plan = build_swap_operation_plan(&component, &artifact)
            .expect("plan")
            .with_prerequisite_blocker(OperationPlanBlocker::DeveloperModeRequired)
            .with_prerequisite_blocker(OperationPlanBlocker::DeveloperModeCheckUnavailable);

        let wire = serde_json::to_value(SwapPlanOutput::from(&plan)).expect("plan json");

        assert_eq!(
            wire["blockers"],
            json!([
                "developer_mode_required",
                "developer_mode_check_unavailable"
            ])
        );
        assert_eq!(wire["risk_level"], "blocked");
    }

    #[test]
    fn candidate_wire_nests_catalog_identity_and_keeps_technical_version_explicit() {
        let output = CandidateOutput {
            artifact_id: "artifact-1".to_owned(),
            file_name: "d3d12.dll".to_owned(),
            file_path: Some("C:/Game/d3d12.dll".to_owned()),
            technical_version: Some("10.1.0".to_owned()),
            release_label: Some("SDK".to_owned()),
            source_game_id: None,
            comparison: "newer_version".to_owned(),
            catalog_package: Some(CatalogCandidateOutput {
                package_id: "microsoft.dxc".to_owned(),
                release: PackageRelease {
                    version: PackageVersion::parse("1.9.2602.17").expect("package version"),
                    channel: ReleaseChannel::Stable,
                    label: Some("SDK".to_owned()),
                },
                availability: CatalogPackageAvailability::Available,
                automatic_selection_allowed: true,
            }),
            is_downloaded: false,
            is_debug: false,
            sha256: "sha256".to_owned(),
            d3d12_executable_action: None,
        };

        let wire = serde_json::to_value(output).expect("candidate json");
        assert_eq!(
            wire,
            json!({
                "artifact_id": "artifact-1",
                "file_name": "d3d12.dll",
                "file_path": "C:/Game/d3d12.dll",
                "technical_version": "10.1.0",
                "release_label": "SDK",
                "source_game_id": null,
                "comparison": "newer_version",
                "catalog_package": {
                    "package_id": "microsoft.dxc",
                    "release": {
                        "version": "1.9.2602.17",
                        "channel": "stable",
                        "label": "SDK",
                    },
                    "availability": "available",
                    "automatic_selection_allowed": true,
                },
                "is_downloaded": false,
                "is_debug": false,
                "sha256": "sha256",
                "d3d12_executable_action": null,
            })
        );
        assert!(wire.get("catalog_package_id").is_none());
    }

    #[test]
    fn installed_release_variants_serialize_to_stable_wire_shapes() {
        // Application state → output → JSON in one path: both the From mapping and the
        // wire contract must stay locked together.
        let cases = [
            (
                InstalledReleaseState::Known {
                    technical_version: Some(Version::parse("2.9.0").expect("version")),
                    release_label: Some("revision b".to_owned()),
                    catalog_release: None,
                },
                json!({
                    "kind": "known",
                    "technical_version": "2.9.0",
                    "release_label": "revision b",
                    "catalog_release": null,
                }),
            ),
            (
                InstalledReleaseState::Known {
                    technical_version: Some(Version::parse("2.8.0").expect("version")),
                    release_label: None,
                    catalog_release: None,
                },
                json!({
                    "kind": "known",
                    "technical_version": "2.8.0",
                    "release_label": null,
                    "catalog_release": null,
                }),
            ),
            (
                InstalledReleaseState::Known {
                    technical_version: None,
                    release_label: Some("SDK".to_owned()),
                    catalog_release: Some(PackageRelease {
                        version: PackageVersion::parse("1.721.2-preview").expect("package version"),
                        channel: ReleaseChannel::Preview,
                        label: Some("SDK".to_owned()),
                    }),
                },
                json!({
                    "kind": "known",
                    "technical_version": null,
                    "release_label": "SDK",
                    "catalog_release": {
                        "version": "1.721.2-preview",
                        "channel": "preview",
                        "label": "SDK",
                    },
                }),
            ),
            (
                InstalledReleaseState::Mixed {
                    min_technical_version: Version::parse("2.4.0").expect("version"),
                    max_technical_version: Version::parse("2.9.0").expect("version"),
                },
                json!({
                    "kind": "mixed",
                    "min_technical_version": "2.4.0",
                    "max_technical_version": "2.9.0",
                }),
            ),
            (InstalledReleaseState::Unknown, json!({ "kind": "unknown" })),
        ];

        for (domain, expected_wire) in cases {
            let wire =
                serde_json::to_value(InstalledReleaseStateOutput::from(&domain)).expect("json");
            assert_eq!(wire, expected_wire);
        }
    }

    #[test]
    fn executable_action_wire_does_not_duplicate_the_plan_token() {
        let profile = D3d12ExecutableProfile::new(
            PathRef::new("C:/Game/game.exe").expect("executable"),
            PathRef::new("C:/Game/game.exe.renderpilot.bak").expect("backup"),
            606,
            606,
            true,
            false,
        );
        let action = D3d12ExecutableAction::for_swap(&profile, 619).expect("action");

        let wire = serde_json::to_value(D3d12ExecutableActionOutput::from(&action)).expect("json");

        assert_eq!(
            wire,
            json!({
                "kind": "patch",
                "executable_path": "C:/Game/game.exe",
                "backup_path": "C:/Game/game.exe.renderpilot.bak",
                "backup_exists": true,
                "original_sdk_version": 606,
                "current_sdk_version": 606,
                "target_sdk_version": 619,
                "requires_confirmation": true,
            })
        );
        assert!(wire.get("confirmation_token").is_none());
    }

    #[test]
    fn executable_status_wire_reports_whether_the_backup_really_exists() {
        let status = D3d12ExecutableStatus {
            component_id: ComponentId::new("component:d3d12").expect("component"),
            executable_path: PathRef::new("C:/Game/game.exe").expect("executable"),
            backup_path: PathRef::new("C:/Game/game.exe.bak").expect("backup"),
            original_sdk_version: 606,
            current_sdk_version: 606,
            backup_exists: false,
            repair_required: false,
            selection_locked: false,
        };

        let wire = serde_json::to_value(D3d12ExecutableStatusOutput::from(&status)).expect("json");

        assert_eq!(
            wire,
            json!({
                "status": "original",
                "selection_locked": false,
                "executable_path": "C:/Game/game.exe",
                "backup_path": "C:/Game/game.exe.bak",
                "backup_exists": false,
                "original_sdk_version": 606,
                "current_sdk_version": 606,
            })
        );
    }

    #[test]
    fn operation_summary_emits_only_the_current_typed_metadata_schema() {
        let current = operation_summary_with_metadata(
            r#"{
                "game_name":"Example",
                "library":"d3d12_agility",
                "from_version":"1.606.4",
                "to_version":"1.619.1",
                "d3d12_executable_action":{
                    "kind":"patch",
                    "executable_path":"C:/Game/game.exe",
                    "from_sdk_version":606,
                    "to_sdk_version":619,
                    "original_sdk_version":606
                }
            }"#,
        );
        assert_eq!(
            serde_json::to_value(&current)
                .expect("summary json")
                .get("metadata")
                .cloned(),
            Some(json!({
                "game_name": "Example",
                "library": "d3d12_agility",
                "from_version": "1.606.4",
                "to_version": "1.619.1",
                "d3d12_executable_action": {
                    "kind": "patch",
                    "executable_path": "C:/Game/game.exe",
                    "from_sdk_version": 606,
                    "to_sdk_version": 619,
                    "original_sdk_version": 606,
                },
            }))
        );

        let legacy = operation_summary_with_metadata(r#"{"library":"dlss"}"#);
        assert!(
            legacy.metadata.is_none(),
            "partial legacy JSON must not leak an untyped wire shape"
        );

        let future = operation_summary_with_metadata(
            r#"{
                "game_name":"Example",
                "library":"dlss",
                "from_version":null,
                "to_version":"3.7.20",
                "unexpected":"future-field"
            }"#,
        );
        assert!(
            future.metadata.is_none(),
            "unknown fields must not silently widen the current contract"
        );
    }

    fn operation_summary_with_metadata(metadata: &str) -> OperationSummaryOutput {
        let operation = OperationRecord::new(
            OperationId::new("operation:typed-metadata").expect("operation id"),
            GameId::new("manual:typed-metadata").expect("game id"),
            OperationKind::ReplaceComponent,
            OperationStatus::Completed,
            UnixTimestampMillis::EPOCH,
        )
        .with_metadata_json(MetadataJson::new(metadata).expect("valid metadata json"));
        OperationSummaryOutput::from(&OperationListCatalogEntry {
            operation,
            item_count: 1,
            component_ids: vec!["component:typed-metadata".to_owned()],
        })
    }
}
