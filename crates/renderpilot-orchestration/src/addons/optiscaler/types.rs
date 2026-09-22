//! Strict catalogue wire types and lifecycle DTOs for OptiScaler.
//!
//! The public field contract is the versioned JSON/IPC schema itself. Keeping
//! field names identical across Rust, the CDN manifest, and TypeScript makes
//! schema review and compatibility diffs mechanical.

use std::collections::BTreeMap;
use std::ops::Deref;

use renderpilot_domain::{
    AddonKind, GraphicsApi, LibraryTechnology as GraphicsTechnology, OptiScalerAdoptionState,
    OptiScalerInstallState,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
/// Strictly decoded external manifest before semantic validation.
pub struct WireOptiScalerManifest {
    /// Wire schema version.
    pub schema_version: u32,
    /// Immutable catalogue revision.
    pub revision: String,
    /// Release identifier selected as stable.
    pub current_release: String,
    /// Reviewed immutable releases.
    pub releases: Vec<OptiScalerRelease>,
    /// Available module definitions.
    pub modules: Vec<OptiScalerModule>,
    /// Supported configuration-schema migrations.
    pub config_migrations: Vec<ConfigKeyMigration>,
}

/// Semantically validated immutable OptiScaler catalogue.
///
/// Construction is deliberately limited to `TryFrom<WireOptiScalerManifest>`;
/// lifecycle code cannot accidentally consume a merely deserialized document.
#[derive(Debug, Clone)]
pub struct OptiScalerManifest {
    wire: WireOptiScalerManifest,
}

impl TryFrom<WireOptiScalerManifest> for OptiScalerManifest {
    type Error = crate::ServiceError;

    fn try_from(wire: WireOptiScalerManifest) -> Result<Self, Self::Error> {
        super::manifest_store::validation::validate(&wire)?;
        Ok(Self { wire })
    }
}

impl Deref for OptiScalerManifest {
    type Target = WireOptiScalerManifest;

    fn deref(&self) -> &Self::Target {
        &self.wire
    }
}

impl WireOptiScalerManifest {
    pub(crate) fn current_release(&self) -> Option<&OptiScalerRelease> {
        self.releases
            .iter()
            .find(|release| release.id == self.current_release)
    }
}

#[cfg(test)]
impl OptiScalerManifest {
    pub(crate) fn wire_clone(&self) -> WireOptiScalerManifest {
        self.wire.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// One immutable OptiScaler release archive.
pub struct OptiScalerRelease {
    /// Stable release identifier.
    pub id: String,
    /// Official upstream transport. This is deliberately separate from the
    /// immutable release identity (`id`, digest, size and member map).
    pub source: OptiScalerReleaseSource,
    /// Expected SHA-256 of the complete archive.
    pub archive_sha256: String,
    /// Expected archive size in bytes.
    pub archive_size: u64,
    /// Configuration schema shipped by this release.
    pub config_schema: u32,
    /// Reviewed archive members.
    pub members: Vec<OptiScalerArchiveMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// Official upstream location of a release asset.
pub struct OptiScalerReleaseSource {
    /// Transport provider.
    pub provider: OptiScalerReleaseProvider,
    /// Provider-specific repository identifier.
    pub repository: String,
    /// Exact release tag.
    pub tag: String,
    /// Exact asset file name.
    pub asset: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
/// Supported official release transports.
pub enum OptiScalerReleaseProvider {
    /// A GitHub release asset.
    GithubRelease,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// One reviewed file inside a release archive.
pub struct OptiScalerArchiveMember {
    /// Normalized path inside the archive.
    pub archive_path: String,
    /// Install-relative destination, or the reserved `$proxy` target.
    pub target: String,
    /// Expected uncompressed SHA-256.
    pub sha256: String,
    /// Expected uncompressed size in bytes.
    pub size: u64,
    /// Owning module identifier.
    pub module: String,
    /// Whether the member must be a valid x64 PE image.
    #[serde(default)]
    pub pe_x64: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// Manifest definition of one selectable module.
pub struct OptiScalerModule {
    /// Stable module identifier.
    pub id: String,
    /// Whether users may deselect the module.
    pub optional: bool,
    /// Whether a fresh install selects the module by default.
    pub bundled_by_default: bool,
    /// Module identifiers required by this module.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Module identifiers incompatible with this module.
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// Optional independently distributed module payload. Bundled release
    /// members take precedence when a future core package includes the module.
    #[serde(default)]
    pub artifact: Option<OptiScalerModuleArtifact>,
    /// Human-readable module description.
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// Independently distributed immutable module artifact.
pub struct OptiScalerModuleArtifact {
    /// Immutable metadata identity for the reviewed upstream bytes.
    pub id: String,
    /// Official artifact source.
    pub source: OptiScalerReleaseSource,
    /// Expected artifact SHA-256.
    pub sha256: String,
    /// Expected artifact size in bytes.
    pub size: u64,
    /// Install-relative destination.
    pub target: String,
    /// Whether the artifact must be a valid x64 PE image.
    pub pe_x64: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// One INI value managed by compatibility or module policy.
pub struct ManagedIniValue {
    /// INI section.
    pub section: String,
    /// Key within the section.
    pub key: String,
    /// Required value.
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// One semantic configuration-key migration.
pub struct ConfigKeyMigration {
    /// Source schema version.
    pub from_schema: u32,
    /// Destination schema version.
    pub to_schema: u32,
    /// Source INI section.
    pub from_section: String,
    /// Source INI key.
    pub from_key: String,
    /// Destination INI section.
    pub to_section: String,
    /// Destination INI key.
    pub to_key: String,
    /// Optional semantic value conversion. When non-empty, an unmapped user
    /// value is preserved at the old key and reported as a merge conflict
    /// instead of being written under the new key with invalid semantics.
    #[serde(default)]
    pub value_map: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// Stable UI-facing reason why compatibility evaluation blocked an operation.
pub enum OptiScalerCompatibilityBlockCode {
    /// The selected executable is not known to be 64-bit.
    RequiresX64,
    /// The game does not expose a supported graphics API.
    UnsupportedGraphicsApi,
    /// A catalogue rule explicitly marks the game unsupported.
    CatalogUnsupported,
    /// No compatible upscaling input was detected.
    InputNotDetected,
    /// The release catalogue has no installable current release.
    ReleaseUnavailable,
    /// The selected component set cannot be provisioned.
    SelectedModulesUnavailable,
    /// Exact catalogue identities or variants conflict.
    CatalogIdentityConflict,
    /// A selected compatibility variant requires Luma Framework first.
    LumaPrerequisite,
    /// The requested proxy slot conflicts with the current host topology.
    ProxyConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// Stable reason why an otherwise valid relocation target cannot be used.
pub enum OptiScalerRelocationBlockCode {
    /// Moving to another runtime directory cannot preserve an active peer's
    /// complete file receipt with the exact evidence currently persisted.
    ActivePeerCrossTargetUnsupported,
}

#[derive(Debug, Clone, Serialize)]
/// Detected graphics-input evidence contributing to eligibility.
pub(crate) struct OptiScalerEvidence {
    /// Detected graphics technology.
    pub technology: GraphicsTechnology,
    /// Files that established the evidence.
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// Compatibility knowledge is distinct from files detected in the game.
pub enum OptiScalerCompatibilityStatus {
    /// The exact game is listed as working.
    Working,
    /// The exact game is listed but has a prerequisite or condition.
    Conditional,
    /// No exact catalogue entry exists; detected files remain real evidence.
    Untested,
    /// The exact game is marked unsupported.
    Unsupported,
}

/// Exact upscaling inputs declared by a matched compatibility entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OptiScalerDeclaredInput {
    /// NVIDIA DLSS Super Resolution 2 or newer.
    Dlss2Plus,
    /// AMD FSR 2 or newer.
    Fsr2Plus,
    /// Intel XeSS.
    Xess,
}

#[derive(Debug, Clone, Serialize)]
/// Reviewed, localizable guidance from the independent compatibility catalogue.
pub struct OptiScalerCompatibilityGuidance {
    /// Stable guidance category.
    pub kind: String,
    /// Stable localization id and reviewed English fallback.
    pub message: crate::addons::catalog_message::CatalogMessage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// Whether reviewed launch arguments are mandatory or advisory.
pub enum OptiScalerLaunchRequirement {
    /// The game must be started with these arguments.
    Required,
    /// These arguments are recommended by the reviewed compatibility source.
    Recommended,
}

#[derive(Debug, Clone, Serialize)]
/// Exact reviewed launch configuration for the selected compatibility variant.
pub struct OptiScalerLaunch {
    /// Safe, independently copyable launcher arguments in their reviewed order.
    pub arguments: Vec<String>,
    /// Whether the arguments are required or recommended.
    pub requirement: OptiScalerLaunchRequirement,
}

#[derive(Debug, Clone, Serialize)]
/// Compatibility projection with no fabricated physical evidence.
pub struct OptiScalerCompatibility {
    /// Exact catalogue status, or `unknown` when unmatched.
    pub status: OptiScalerCompatibilityStatus,
    /// Inputs declared by the exact matched catalogue entry, in reviewed
    /// catalogue order. Unknown/conflicting matches expose no declaration.
    pub declared_inputs: Vec<OptiScalerDeclaredInput>,
    /// Selected typed launch configuration, when the exact variant needs one.
    pub launch: Option<OptiScalerLaunch>,
    /// Manually curated display guidance.
    pub guidance: Vec<OptiScalerCompatibilityGuidance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// Current state of a prerequisite required for a new install.
pub enum OptiScalerPrerequisiteState {
    /// No prerequisite is required by the selected compatibility variant.
    None,
    /// Luma Framework's durable payload is present.
    Satisfied,
    /// RenoDX owns the peer slot and must be removed before Luma can be installed.
    RemoveRenoDx,
    /// A prior Luma install was interrupted.
    LumaTorn,
    /// Luma has a durable record but its payload is not intact.
    LumaBroken,
    /// Luma can be installed for this game before OptiScaler.
    InstallLuma,
    /// Luma is unavailable for this game.
    LumaUnavailable,
}

#[derive(Debug, Clone, Serialize)]
/// Public proxy-chain plan for the selected target.
pub struct OptiScalerProxyPlan {
    /// Root proxy slot loaded by the game.
    pub slot: String,
    /// Whether a ReShade host will run downstream.
    pub chain_reshade: bool,
    /// Downstream host path when chaining is planned.
    pub downstream_path: Option<String>,
    /// Blocking topology conflict, if any.
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
/// Availability and dependency projection for one module.
pub struct OptiScalerModuleAvailability {
    /// Stable module identifier.
    pub id: String,
    /// Whether the current selection includes the module.
    pub selected: bool,
    /// Whether the module may be deselected.
    pub optional: bool,
    /// Whether all policy and provisioning requirements are satisfied.
    pub available: bool,
    /// Selected source of module bytes.
    pub provisioning: OptiScalerModuleProvisioning,
    /// Required module identifiers.
    pub requires: Vec<String>,
    /// Conflicting module identifiers.
    pub conflicts: Vec<String>,
    /// Human-readable module description.
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// How an available module will be provisioned.
pub enum OptiScalerModuleProvisioning {
    /// Included in the core release archive.
    Bundled,
    /// Downloaded from a manifest-pinned artifact.
    PinnedArtifact,
    /// Reuses a compatible library already present in the game.
    ExistingGame,
    /// Obtained from RenderPilot's reviewed component catalogue.
    CatalogDownload,
    /// No safe source is available.
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
/// Compact managed-install projection intended for availability and desktop UI.
pub struct OptiScalerInstallSummary {
    /// Installed immutable release identifier.
    pub release_id: String,
    /// Executable currently serviced by the managed installation.
    pub target_exe_path: String,
    /// Directory containing the managed runtime.
    pub target_dir: String,
    /// Stable selected module identifiers.
    pub modules: Vec<String>,
    /// How the installation entered managed lifecycle state.
    pub adoption_state: OptiScalerAdoptionState,
}

impl From<&OptiScalerInstallState> for OptiScalerInstallSummary {
    fn from(state: &OptiScalerInstallState) -> Self {
        Self {
            release_id: state.release_id.clone(),
            target_exe_path: state.target_exe_path.as_str().to_owned(),
            target_dir: state.target_dir.as_str().to_owned(),
            modules: state.modules.clone(),
            adoption_state: state.adoption_state,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
/// Complete UI-facing availability report for one game.
pub struct OptiScalerAvailability {
    /// Stable game identifier.
    pub game_id: String,
    /// Launcher that starts the selected game installation.
    pub launcher: renderpilot_domain::Launcher,
    /// Whether the selected operation target passes every blocking check.
    pub available: bool,
    /// Primary blocking reason.
    pub blocked_reason: Option<String>,
    /// Stable blocking category suitable for localized UI copy.
    pub compatibility_block_code: Option<OptiScalerCompatibilityBlockCode>,
    /// Graphics APIs detected for the selected executable.
    pub detected_apis: Vec<GraphicsApi>,
    /// Exact compatibility knowledge and reviewed guidance.
    pub compatibility: OptiScalerCompatibility,
    /// Current prerequisite state for a new compatibility-gated install.
    pub prerequisite: OptiScalerPrerequisiteState,
    /// Current stable manifest release.
    pub selected_release: Option<String>,
    /// Effective executable serviced by lifecycle operations.
    pub target_exe: Option<String>,
    /// Newly selected effective executable, offered only as an explicit
    /// relocation destination while lifecycle operations keep servicing the
    /// recorded install target.
    pub relocation_target_exe: Option<String>,
    /// Blocking reason specific to the offered relocation target.
    pub relocation_blocked_reason: Option<String>,
    /// Stable blocking category for the offered relocation target.
    pub relocation_block_code: Option<OptiScalerRelocationBlockCode>,
    /// Directory receiving the OptiScaler runtime.
    pub target_dir: Option<String>,
    /// Public proxy-chain plan.
    pub proxy: OptiScalerProxyPlan,
    /// Module availability projection.
    pub modules: Vec<OptiScalerModuleAvailability>,
    /// Persisted lifecycle state, when managed.
    pub install_state: Option<OptiScalerInstallSummary>,
    /// Managed paths whose current bytes differ from receipts.
    pub drifted_paths: Vec<String>,
    /// Whether a newer immutable release is selected.
    pub update_available: bool,
    /// Whether drift or module reconciliation requires repair.
    pub repair_required: bool,
    /// Whether recognizable OptiScaler files exist without managed state.
    pub unmanaged: bool,
    /// Whether current policy may be applied to a persisted installation.
    pub maintenance_available: bool,
    /// Why policy maintenance is unavailable, when a stable code is useful.
    pub maintenance_block_code: Option<OptiScalerCompatibilityBlockCode>,
}

#[derive(Debug, Clone, Serialize)]
/// Result of an OptiScaler filesystem operation.
pub struct OptiScalerOperationResult {
    /// Add-on kind changed by the operation.
    pub kind: AddonKind,
    /// Resulting managed state, or `None` after uninstall.
    pub state: Option<OptiScalerInstallSummary>,
    /// Paths changed by the operation.
    pub changed_paths: Vec<String>,
    /// User-modified paths intentionally preserved.
    pub preserved_paths: Vec<String>,
    /// Semantic configuration conflicts requiring review.
    pub config_conflicts: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
/// Compact update and drift verdict for one managed installation.
pub struct OptiScalerUpdateCheck {
    /// Standard add-on update verdict used by shared CLI and desktop flows.
    pub overall: crate::addons::update::UpdateStatus,
    /// Installed immutable release identifier.
    pub installed_release: Option<String>,
    /// Stable manifest release identifier.
    pub available_release: Option<String>,
    /// Whether the stable release differs from the installed release.
    pub update_available: bool,
    /// Whether managed-file or module drift requires repair.
    pub repair_required: bool,
    /// Drifted managed paths.
    pub drifted_paths: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_install_summary_omits_durable_receipt_internals() {
        let value = serde_json::to_value(OptiScalerInstallSummary {
            release_id: "stable".to_owned(),
            target_exe_path: "C:/Game/game.exe".to_owned(),
            target_dir: "C:/Game".to_owned(),
            modules: vec!["core".to_owned()],
            adoption_state: OptiScalerAdoptionState::Managed,
        })
        .expect("serializable summary");

        for internal in [
            "release_files",
            "manifest_revision",
            "proxy_topology_id",
            "config_schema",
            "config_base_release",
            "created_at",
            "updated_at",
        ] {
            assert!(value.get(internal).is_none(), "leaked {internal}");
        }
    }
}
