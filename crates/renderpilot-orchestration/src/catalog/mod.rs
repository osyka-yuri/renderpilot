//! Catalog orchestration: scan, query, and library management.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use renderpilot_application::{
    AppError, AppResult, ArtifactRepository, CandidateArtifactIndex,
    ComponentReplacementCandidates, ComponentRepository, CoordinatedCandidateOption,
    GameRepository, InstalledAddonRepository, OperationPlan, OperationRecord,
    find_replacement_candidate_selection_indexed,
};
use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::{
    AddonKind, ComponentFile, GameId, GameInstallation, LibraryArtifact, LibraryComponent,
    LibraryTechnology,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::ServiceError;

/// Game install root for a mutation, falling back to an owned-path parent when
/// the catalog row was pruned but add-on debris remains on disk.
pub(crate) fn game_root_for_mutation(
    storage: &SqliteStorage,
    game_id: &GameId,
    fallback_parent: Option<PathBuf>,
) -> AppResult<PathBuf> {
    storage
        .find_game(game_id)?
        .map(|game| PathBuf::from(game.install_path().as_str()))
        .or(fallback_parent)
        .ok_or_else(|| AppError::game_not_found(game_id.as_str()))
}

mod add_game;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactPeProfileMismatch {
    MissingDeclared,
    MissingObserved,
    Different,
}

pub(crate) fn validate_exact_pe_profile(
    technology: LibraryTechnology,
    declared: &ComponentFile,
    observed: &renderpilot_detection::PeInspection,
) -> Result<(), ExactPeProfileMismatch> {
    if !technology.requires_exact_pe_profile() {
        return Ok(());
    }
    let declared = declared
        .pe_compatibility()
        .ok_or(ExactPeProfileMismatch::MissingDeclared)?;
    let observed = observed
        .compatibility_profile()
        .ok_or(ExactPeProfileMismatch::MissingObserved)?;
    if declared != &observed {
        return Err(ExactPeProfileMismatch::Different);
    }
    Ok(())
}
#[cfg(windows)]
pub mod auto_scan;
mod cards;
pub(crate) mod cascade;
mod developer_mode;
pub mod execute;
mod install_boundary;
mod install_paths;
mod managed_state;
mod operations;
pub mod output;
mod read_service;
mod recovery_bundle;
mod remove_game;
mod root_correction;
mod runtime_compatibility;
/// Auto-discovery and scanning.
pub mod scan;
mod source_assessment;
mod swap;
#[cfg(test)]
mod test_support;

/// The game installation and detected library files produced by a folder scan.
pub struct ScanFolderCatalogResult {
    /// The game installation discovered at the scanned path.
    pub game: GameInstallation,
    /// Library files detected within the game installation.
    pub libraries: Vec<DetectedLibraryFile>,
    /// How persisted card facts changed compared with the previous scan.
    pub change: CatalogScanChange,
    /// Proven legacy-card consolidation performed with this scan.
    pub consolidation: ScanConsolidationOutcome,
    /// Durable snapshot created before root correction archived operation
    /// history outside the corrected installation boundary.
    pub root_correction_recovery_bundle_path: Option<String>,
}

/// Observable state migration performed by a full installation scan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanConsolidationOutcome {
    /// False legacy cards removed after one-to-one component proof.
    pub removed_game_ids: Vec<GameId>,
    /// Candidate legacy cards retained because proof was insufficient.
    pub retained_candidate_game_ids: Vec<GameId>,
    /// Tables whose destination rows won a key conflict.
    pub destination_wins_conflicts: Vec<String>,
    /// Durable recovery bundle created before a lossy conflict.
    pub recovery_bundle_path: Option<String>,
}

/// Identity-level outcome for one scanned installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogScanChange {
    /// The install already existed and its card facts were identical.
    Unchanged,
    /// A new catalog installation was persisted.
    Added,
    /// An existing installation's catalog facts changed.
    Updated,
}

/// Stable identity delta produced by a complete scan session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogScanDelta {
    /// Newly discovered game ids.
    pub added_game_ids: Vec<GameId>,
    /// Existing game ids whose persisted catalog facts changed.
    pub updated_game_ids: Vec<GameId>,
    /// Catalog game ids removed as stale within the scanned scope.
    pub removed_game_ids: Vec<GameId>,
}

impl CatalogScanDelta {
    /// All added and updated ids in stable order, excluding removals.
    #[must_use]
    pub fn changed_game_ids(&self) -> Vec<GameId> {
        let removed = self.removed_game_ids.iter().collect::<HashSet<_>>();
        self.added_game_ids
            .iter()
            .chain(&self.updated_game_ids)
            .filter(|game_id| !removed.contains(game_id))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

/// Game id and replacement candidate groups for a component swap UI.
pub struct CandidateCatalogResult {
    /// The game id the candidates belong to.
    pub game_id: GameId,
    /// Grouped replacement candidates, one group per component.
    pub groups: Vec<ComponentReplacementCandidates>,
}

/// Full game details for the main detail view.
#[derive(Debug, Clone)]
pub struct GameDetailsCatalogResult {
    /// The game installation.
    pub game: GameInstallation,
    /// All library components for this game.
    pub components: Vec<LibraryComponent>,
    /// Component ids with a currently usable rollback baseline.
    pub backup_component_ids: HashSet<String>,
    /// Replacement candidate groups across all components.
    pub candidate_groups: Vec<ComponentReplacementCandidates>,
    /// Backend-coordinated Streamline manual options.
    pub streamline_candidate_options: Vec<CoordinatedCandidateOption>,
    /// Fresh active D3D12 executable status, independent of candidate availability.
    pub d3d12_executable_status: Option<D3d12ExecutableStatus>,
    /// Operation history for this game.
    pub operations: OperationListCatalogResult,
    /// Profile-derived and currently installed add-on capabilities.
    pub addon_capabilities: Vec<AddonKind>,
}

/// Fresh read-only D3D12 executable status for the game details view.
#[derive(Debug, Clone)]
pub struct D3d12ExecutableStatus {
    component_id: renderpilot_domain::ComponentId,
    executable_path: renderpilot_domain::PathRef,
    backup_path: renderpilot_domain::PathRef,
    original_sdk_version: u32,
    current_sdk_version: u32,
    backup_exists: bool,
    repair_required: bool,
    selection_locked: bool,
}

impl D3d12ExecutableStatus {
    /// Component whose D3D12 runtime owns this executable assessment.
    pub const fn component_id(&self) -> &renderpilot_domain::ComponentId {
        &self.component_id
    }

    /// Active executable path.
    pub const fn executable_path(&self) -> &renderpilot_domain::PathRef {
        &self.executable_path
    }

    /// Expected immutable sidecar path, whether or not it currently exists.
    pub const fn backup_path(&self) -> &renderpilot_domain::PathRef {
        &self.backup_path
    }

    /// SDK line from the immutable original.
    pub const fn original_sdk_version(&self) -> u32 {
        self.original_sdk_version
    }

    /// SDK line currently active.
    pub const fn current_sdk_version(&self) -> u32 {
        self.current_sdk_version
    }

    /// Whether the immutable original sidecar currently exists.
    pub const fn backup_exists(&self) -> bool {
        self.backup_exists
    }

    /// Whether an external executable change requires repair.
    pub const fn repair_required(&self) -> bool {
        self.repair_required
    }

    /// Whether an aggregate permanently binds executable selection.
    pub const fn selection_locked(&self) -> bool {
        self.selection_locked
    }
}

/// Resolved swap operation plan ready for execution.
pub struct SwapPlanCatalogResult {
    /// The resolved operation plan.
    pub plan: OperationPlan,
}

/// Operation history for a game.
#[derive(Debug, Clone)]
pub struct OperationListCatalogResult {
    /// The game id the operations belong to.
    pub game_id: GameId,
    /// Ordered list of operation entries.
    pub operations: Vec<OperationListCatalogEntry>,
}

/// A single entry in the operation history list.
#[derive(Debug, Clone)]
pub struct OperationListCatalogEntry {
    /// The operation record.
    pub operation: OperationRecord,
    /// Number of items (files) affected by the operation.
    pub item_count: usize,
    /// String ids of the components affected.
    pub component_ids: Vec<String>,
}

/// Computes the add-on capabilities (RenoDX / Luma) for a single game using the
/// same rules as the catalog cards list: profile snapshot (from manifests) union
/// any currently active installed add-on for the game.
pub fn addon_capabilities(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<Vec<AddonKind>, ServiceError> {
    let profile =
        crate::addons::capabilities::DurableProfileCapabilities::load_for_game(context, game_id)?;
    let installed =
        crate::addons::records::active_record(context, game_id)?.map(|record| record.kind());
    Ok(merge_addon_capabilities(&profile, installed))
}

/// Merge profile-derived capabilities (from manifest matchers in the snapshot)
/// with any currently active installed add-on for the game.
///
/// A game appears to "support" an add-on (and therefore gets a card / badge /
/// filter option) if *either* the profile snapshot says the manifest matches
/// *or* the game has an active record in `installed_addons`.
pub(crate) fn merge_addon_capabilities(
    profile_capabilities: &[AddonKind],
    installed: Option<AddonKind>,
) -> Vec<AddonKind> {
    AddonKind::ALL
        .iter()
        .copied()
        .filter(|kind| profile_capabilities.contains(kind) || installed == Some(*kind))
        .collect()
}

/// Returns all game installations stored in the catalog.
pub fn list_games(context: &crate::Context) -> Result<Vec<GameInstallation>, ServiceError> {
    context.storage().list_games().map_err(Into::into)
}

pub use root_correction::{
    RootCorrectionAssessment, RootCorrectionBlockerKind, RootCorrectionCleanupAction,
    RootCorrectionStatus,
};

/// The per-query-constant inputs for replacement-candidate matching: the local +
/// catalog artifact universe and the candidate context.
///
/// Loading these is independent of the game, so a multi-game caller (the
/// catalog snapshot builder builds this **once** and reuses it for every game
/// instead of re-reading the artifacts table and catalog snapshot per game.
#[derive(Debug)]
pub(crate) struct ReplacementUniverse {
    artifact_index: CandidateArtifactIndex,
    candidate_context: renderpilot_application::CandidateContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReplacementUniverseRevision {
    inventory: u64,
    catalog: Option<(u64, u128)>,
    local_files: u64,
}

/// Loads the artifact universe (local artifacts + catalog packages) and the
/// candidate context once. A missing or invalid catalog degrades to local-only
/// artifacts.
pub(crate) fn load_replacement_universe(
    context: &crate::Context,
) -> Result<Arc<ReplacementUniverse>, ServiceError> {
    let mut inventory_revision = context.storage().library_artifact_revision()?;
    let mut catalog_revision = crate::libraries::replacement_catalog_revision()?;
    if let Some(cached) =
        current_replacement_universe(context, inventory_revision, catalog_revision)
    {
        return Ok(cached);
    }

    let _rebuild = context.replacement_universe_rebuild_guard();
    inventory_revision = context.storage().library_artifact_revision()?;
    catalog_revision = crate::libraries::replacement_catalog_revision()?;
    if let Some(cached) =
        current_replacement_universe(context, inventory_revision, catalog_revision)
    {
        return Ok(cached);
    }

    let (artifacts, downloaded_ids, active_catalog) =
        crate::libraries::replacement_artifacts(context)?;
    let candidate_context =
        renderpilot_application::CandidateContext::new(downloaded_ids, active_catalog);

    let universe = Arc::new(ReplacementUniverse {
        artifact_index: CandidateArtifactIndex::new(artifacts),
        candidate_context,
    });
    let effective_revision = ReplacementUniverseRevision {
        inventory: context.storage().library_artifact_revision()?,
        catalog: crate::libraries::replacement_catalog_revision()?,
        local_files: local_artifact_metadata_revision(universe.artifact_index.artifacts()),
    };
    context.cache_replacement_universe(effective_revision, Arc::clone(&universe));
    Ok(universe)
}

fn current_replacement_universe(
    context: &crate::Context,
    inventory_revision: u64,
    catalog_revision: Option<(u64, u128)>,
) -> Option<Arc<ReplacementUniverse>> {
    let (cached_revision, cached) = context.replacement_universe_cache()?;
    (cached_revision.inventory == inventory_revision
        && cached_revision.catalog == catalog_revision
        && cached_revision.local_files
            == local_artifact_metadata_revision(cached.artifact_index.artifacts()))
    .then_some(cached)
}

fn local_artifact_metadata_revision(artifacts: &[LibraryArtifact]) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    use std::time::UNIX_EPOCH;

    let mut hasher = DefaultHasher::new();
    for artifact in artifacts {
        artifact.id().as_str().hash(&mut hasher);
        for file in artifact.files() {
            let path = file.path().as_str();
            path.hash(&mut hasher);
            match std::fs::metadata(path) {
                Ok(metadata) => {
                    true.hash(&mut hasher);
                    metadata.len().hash(&mut hasher);
                    metadata
                        .modified()
                        .ok()
                        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                        .map(|value| value.as_nanos())
                        .hash(&mut hasher);
                }
                Err(_) => false.hash(&mut hasher),
            }
        }
    }
    hasher.finish()
}

/// Builds full game details using a pre-loaded [`ReplacementUniverse`].
///
/// Only the genuinely per-game work runs here (components, candidate match,
/// operations); the constant artifact/manifest load is the caller's
/// responsibility via [`load_replacement_universe`].
pub(crate) fn get_game_details_with_universe(
    context: &crate::Context,
    game_id: &GameId,
    universe: &ReplacementUniverse,
) -> Result<GameDetailsCatalogResult, ServiceError> {
    let storage = context.storage();
    let game = storage.require_game(game_id)?;
    let components = storage.list_components_for_game(game_id)?;
    let installed_addon = storage.get_installed_addon(game_id)?;
    let profile_capabilities =
        crate::addons::capabilities::DurableProfileCapabilities::load_for_game(context, game_id)?;
    let addon_capabilities = merge_addon_capabilities(
        &profile_capabilities,
        installed_addon.as_ref().map(|a| a.kind()),
    );
    let backup_component_ids =
        crate::coordinated_files::available_component_backup_ids(storage, game_id, &components)?;

    let d3d12_component = components
        .iter()
        .find(|component| component.technology() == LibraryTechnology::D3D12Agility);
    // Game details are a presentation read model. Reading and hashing complete
    // EXE/backup images here made ordinary navigation scale with executable
    // size. Swap preview/apply and rollback retain their independent,
    // authoritative assessment under the per-game mutation lock.
    let runtime_compatibility::PresentationTargetProfileAssessment {
        profile: target_profile,
        d3d12: d3d12_state,
    } = runtime_compatibility::presentation_target_profile(context, &game, d3d12_component)?;
    let d3d12_executable_status = d3d12_component
        .zip(d3d12_state.as_ref())
        .map(|(component, state)| build_d3d12_status(component, state))
        .transpose()?;
    let candidate_context = universe
        .candidate_context
        .with_target_profile(target_profile);
    let matching_components = components_for_candidate_matching_with_installed(
        game_id,
        &components,
        installed_addon.as_ref(),
    )?;
    let candidate_selection = find_replacement_candidate_selection_indexed(
        &matching_components,
        &universe.artifact_index,
        &candidate_context,
    );
    let (candidate_groups, streamline_candidate_options) = candidate_selection.into_parts();

    let operations = list_operations(context, game_id)?;

    Ok(GameDetailsCatalogResult {
        game,
        components,
        backup_component_ids,
        candidate_groups,
        streamline_candidate_options,
        d3d12_executable_status,
        operations,
        addon_capabilities,
    })
}

fn build_d3d12_status(
    component: &LibraryComponent,
    state: &runtime_compatibility::D3d12ExecutablePresentationState,
) -> AppResult<D3d12ExecutableStatus> {
    Ok(D3d12ExecutableStatus {
        component_id: component.id().clone(),
        executable_path: renderpilot_domain::PathRef::new(
            state.executable_path.to_string_lossy().into_owned(),
        )
        .map_err(|error| AppError::invalid_input(error.to_string()))?,
        backup_path: renderpilot_domain::PathRef::new(
            state.backup_path.to_string_lossy().into_owned(),
        )
        .map_err(|error| AppError::invalid_input(error.to_string()))?,
        original_sdk_version: state.original_sdk_version,
        current_sdk_version: state.current_sdk_version,
        backup_exists: state.backup_exists,
        repair_required: state.repair_required,
        selection_locked: state.selection_locked,
    })
}

/// Refreshes byte-derived state required for safe candidate presentation.
///
/// OpenVR compatibility depends on the currently installed DLL's architecture
/// and complete named-export surface, so persisted scan metadata is not
/// sufficient for a live dropdown. A component that cannot be refreshed is
/// omitted fail-closed. Other technologies retain their existing executable-
/// context policy and avoid an unnecessary per-query file hash.
pub(crate) fn components_for_candidate_matching<'components>(
    context: &crate::Context,
    game_id: &GameId,
    components: &'components [LibraryComponent],
) -> Result<Cow<'components, [LibraryComponent]>, ServiceError> {
    if !components
        .iter()
        .any(|component| component.technology() == LibraryTechnology::OpenVr)
    {
        return Ok(Cow::Borrowed(components));
    }

    let installed_addon = context.storage().get_installed_addon(game_id)?;
    let managed_files = crate::coordinated_files::managed_files_of(installed_addon.as_ref());

    Ok(Cow::Owned(
        components
            .iter()
            .filter_map(|component| {
                if component.technology() != LibraryTechnology::OpenVr {
                    return Some(component.clone());
                }
                match crate::coordinated_files::current_component_snapshot(component, managed_files)
                {
                    Ok(snapshot) => Some(snapshot.into_component()),
                    Err(error) => {
                        log::warn!(
                            "omitting stale OpenVR component {} from replacement candidates: {error}",
                            component.id().as_str()
                        );
                        None
                    }
                }
            })
            .collect(),
    ))
}

/// Re-hashes every active component member for the background card validator.
///
/// External replacement of any DLL invalidates its persisted hash/version
/// facts. Stale components are omitted fail-closed from candidate matching;
/// the original durable component still remains visible on the card.
pub(crate) fn components_for_candidate_matching_with_installed(
    game_id: &GameId,
    components: &[LibraryComponent],
    installed_addon: Option<&renderpilot_domain::InstalledAddon>,
) -> Result<Vec<LibraryComponent>, ServiceError> {
    let managed_files = crate::coordinated_files::managed_files_of(installed_addon);
    Ok(components
        .iter()
        .filter_map(|component| {
            match crate::coordinated_files::current_component_snapshot(component, managed_files) {
                Ok(snapshot) => Some(snapshot.into_component()),
                Err(error) => {
                    log::warn!(
                        "omitting stale component {} from live catalog candidates for {}: {error}",
                        component.id().as_str(),
                        game_id.as_str(),
                    );
                    None
                }
            }
        })
        .collect())
}

/// Returns full game details including components, candidates, and operations.
pub fn get_game_details(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<GameDetailsCatalogResult, ServiceError> {
    let initial_generation = context.storage().catalog_generation();
    if let Some(details) = context.game_details_cache(game_id, initial_generation) {
        return Ok((*details).clone());
    }

    let rebuild_lock = context.game_details_rebuild_lock(game_id);
    let _rebuild = rebuild_lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    loop {
        let catalog_generation = context.storage().catalog_generation();
        if let Some(details) = context.game_details_cache(game_id, catalog_generation) {
            return Ok((*details).clone());
        }
        let universe = load_replacement_universe(context)?;
        let details = get_game_details_with_universe(context, game_id, &universe)?;
        if context.storage().catalog_generation() == catalog_generation {
            context.cache_game_details(
                game_id.clone(),
                catalog_generation,
                Arc::new(details.clone()),
            );
            return Ok(details);
        }
        log::debug!(
            "catalog changed during details build for {}; retrying",
            game_id.as_str()
        );
    }
}

/// Returns library artifacts stored in the catalog, optionally filtered by technology.
pub fn list_artifacts(
    context: &crate::Context,
    technology: Option<LibraryTechnology>,
) -> Result<Vec<LibraryArtifact>, ServiceError> {
    let artifacts = context.storage().list_artifacts()?;
    Ok(filter_artifacts_by_technology(artifacts, technology))
}

// Re-export core operations from sub-modules directly.
pub use add_game::{
    AddGameCatalogAction, AddGameDecision, AddGameDisposition, AddGameInspection, AddGameOption,
    AddGameRequest, AddGameResult, AddGameReview, AddGameRootChoice, AddGameUnavailableReason,
    AddGameWarning, ExecutableInspection, InstallBoundaryEvidence, InstallBoundaryInspection,
    InstallBoundaryKind, InstallRelationship, InstallRelationshipKind,
    RootRecommendationConfidence, RootRecommendationInspection, RootRecommendationSource,
    TraversalCompleteness, add_game, inspect_game_install,
};
pub use cards::{CatalogCardRiskLevel, CatalogRevision, CatalogSnapshot, GameCardData};
pub use execute::{
    D3d12ExecutableActionResult, D3d12ExecutableActionResultKind, RollbackPlan, apply_swap,
    apply_swap_confirmed, build_rollback_plan, rollback_component,
};
pub use operations::list_operations;
pub use read_service::CatalogReadService;
pub use remove_game::{RemoveGameFromCatalogResult, remove_game_from_catalog};
pub use swap::{build_swap_plan, find_candidates};

/// Returns the distinct graphics-technology library tags present in the catalog.
pub fn distinct_game_libraries(context: &crate::Context) -> Result<Vec<String>, ServiceError> {
    context
        .storage()
        .list_distinct_game_libraries()
        .map_err(Into::into)
}

/// Returns the distinct launcher names present in the catalog.
pub fn distinct_game_launchers(context: &crate::Context) -> Result<Vec<String>, ServiceError> {
    context
        .storage()
        .list_distinct_game_launchers()
        .map_err(Into::into)
}

/// Returns the set of component ids that have a rollback backup for `game_id`.
pub fn backup_component_ids(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<HashSet<String>, ServiceError> {
    let storage = context.storage();
    let components = storage.list_components_for_game(game_id)?;
    crate::coordinated_files::available_component_backup_ids(storage, game_id, &components)
        .map_err(Into::into)
}

/// Reads one persisted catalog setting value.
pub fn get_catalog_setting(
    context: &crate::Context,
    key: &str,
) -> Result<Option<String>, ServiceError> {
    context.storage().get_setting(key).map_err(Into::into)
}

/// Upserts a catalog setting, or deletes the row when `value` is blank after trimming.
pub fn set_catalog_setting(
    context: &crate::Context,
    key: &str,
    value: &str,
) -> Result<(), ServiceError> {
    let storage = context.storage();
    if value.trim().is_empty() {
        storage.delete_setting(key).map_err(Into::into)
    } else {
        storage.set_setting(key, value).map_err(Into::into)
    }
}

/// Sets the favorite flag for `game_id`, preserving its hidden flag.
pub fn set_game_favorite(
    context: &crate::Context,
    game_id: &GameId,
    is_favorite: bool,
) -> Result<(), ServiceError> {
    update_game_ui_state(context, game_id, |_, hidden| (is_favorite, hidden))
}

/// Sets the hidden flag for `game_id`, preserving its favorite flag.
pub fn set_game_hidden(
    context: &crate::Context,
    game_id: &GameId,
    is_hidden: bool,
) -> Result<(), ServiceError> {
    update_game_ui_state(context, game_id, |favorite, _| (favorite, is_hidden))
}

/// Reads the current UI state, applies `f` to produce the new
/// `(is_favorite, is_hidden)` pair, and persists it.
fn update_game_ui_state(
    context: &crate::Context,
    game_id: &GameId,
    f: impl FnOnce(bool, bool) -> (bool, bool),
) -> Result<(), ServiceError> {
    let storage = context.storage();
    let generation_before = storage.catalog_generation();
    let current = storage.get_game_ui_state(game_id.as_str())?;
    let (prev_favorite, prev_hidden) = current
        .map(|state| (state.is_favorite, state.is_hidden))
        .unwrap_or((false, false));
    let (is_favorite, is_hidden) = f(prev_favorite, prev_hidden);
    if (is_favorite, is_hidden) == (prev_favorite, prev_hidden) {
        return Ok(());
    }
    storage
        .save_game_ui_state(game_id.as_str(), is_favorite, is_hidden)
        .map_err(ServiceError::from)?;
    let generation_after = storage.catalog_generation();
    context.patch_catalog_ui_state(
        game_id,
        is_favorite,
        is_hidden,
        generation_before,
        generation_after,
    );
    Ok(())
}

fn filter_artifacts_by_technology(
    artifacts: Vec<LibraryArtifact>,
    technology: Option<LibraryTechnology>,
) -> Vec<LibraryArtifact> {
    match technology {
        Some(required_technology) => artifacts
            .into_iter()
            .filter(|artifact| artifact.technology() == required_technology)
            .collect(),
        None => artifacts,
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{
        Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, ComponentFile, ComponentId,
        ComponentKind, InstalledAddon, LibraryArtifact, PathRef, PeCompatibilityProfile,
        PeExportSet, RuntimeTarget, Sha256Hash, Swappability, UpstreamPackage,
        UpstreamPackageProvider, Version,
    };
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn scan_delta_changed_ids_are_sorted_deduplicated_and_exclude_removals() {
        let game_a = GameId::new("manual:a").expect("game a");
        let game_b = GameId::new("manual:b").expect("game b");
        let delta = CatalogScanDelta {
            added_game_ids: vec![game_b.clone(), game_b],
            updated_game_ids: vec![game_a.clone()],
            removed_game_ids: vec![game_a],
        };

        assert_eq!(
            delta.changed_game_ids(),
            vec![GameId::new("manual:b").expect("game b")]
        );
    }

    #[test]
    fn merge_addon_capabilities_unions_profile_with_installed_in_stable_order() {
        // Merge (used by catalog cards and GameDetails) unions profile + installed
        // capabilities and preserves AddonKind::ALL order.
        assert_eq!(
            merge_addon_capabilities(&[AddonKind::Luma], Some(AddonKind::RenoDx)),
            vec![AddonKind::RenoDx, AddonKind::Luma]
        );
    }

    #[test]
    fn addon_capabilities_ignore_a_stale_renodx_record() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context =
            crate::Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:42").expect("game id");
        let addon = game_dir.path().join("renodx-test.addon64");
        let record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(addon.to_string_lossy()).expect("addon path"),
        );
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed record");

        assert!(
            !addon_capabilities(&context, &game_id)
                .expect("capabilities")
                .contains(&AddonKind::RenoDx)
        );

        std::fs::write(addon, b"addon").expect("write payload");
        assert!(
            addon_capabilities(&context, &game_id)
                .expect("capabilities")
                .contains(&AddonKind::RenoDx)
        );
    }

    #[test]
    fn candidate_matching_refreshes_openvr_profile_from_current_bytes() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context =
            crate::Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("game:openvr-freshness").expect("game id");
        let dll = game_dir.path().join(renderpilot_domain::openvr::DLL_NAME);
        std::fs::write(&dll, b"current-but-not-a-pe").expect("write DLL");
        let hash = renderpilot_detection::sha256_file(&dll).expect("hash");
        let profile = PeCompatibilityProfile::new(
            Architecture::X64,
            PeExportSet::from_canonical_names(vec!["VR_InitInternal".into()]).expect("exports"),
        );
        let component = LibraryComponent::new(
            ComponentId::new("component:openvr-freshness").expect("component id"),
            game_id.clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::OpenVr,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(PathRef::new(dll.to_string_lossy()).expect("path"))
                .with_sha256(hash)
                .with_pe_compatibility(profile),
        );

        let components = [component];
        let refreshed =
            components_for_candidate_matching(&context, &game_id, &components).expect("refresh");
        assert_eq!(refreshed.len(), 1);
        assert_eq!(refreshed[0].files()[0].pe_compatibility(), None);

        let candidate_file =
            ComponentFile::new(PathRef::new("catalog://valve/openvr_api.dll").expect("path"))
                .with_sha256(Sha256Hash::new("b".repeat(64)).expect("hash"))
                .with_pe_compatibility(PeCompatibilityProfile::new(
                    Architecture::X64,
                    PeExportSet::from_canonical_names(vec![
                        "VR_InitInternal".into(),
                        "VR_ShutdownInternal".into(),
                    ])
                    .expect("exports"),
                ));
        let metadata = ArtifactMetadata::default()
            .with_release(Version::parse("2.0.0").expect("version"), None)
            .expect("release")
            .with_runtime_target(RuntimeTarget::new(Architecture::X64))
            .with_upstream_package(
                UpstreamPackage::new(
                    UpstreamPackageProvider::GitHub,
                    renderpilot_domain::openvr::UPSTREAM_REPOSITORY,
                    "2.0.0",
                )
                .expect("provenance"),
            );
        let candidate = LibraryArtifact::new(
            ArtifactId::new("artifact:openvr-freshness").expect("artifact id"),
            LibraryTechnology::OpenVr,
            renderpilot_domain::openvr::DLL_NAME,
            vec![candidate_file],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
        .with_metadata(metadata);

        assert!(
            renderpilot_application::find_replacement_candidates(
                &refreshed,
                &[candidate],
                &renderpilot_application::CandidateContext::empty(),
            )
            .is_empty(),
            "an OpenVR DLL without a freshly observed profile must have no dropdown candidates"
        );
    }

    #[test]
    fn live_card_candidate_matching_checks_non_openvr_dll_bytes() {
        let game_dir = tempdir().expect("game dir");
        let game_id = GameId::new("game:dlss-freshness").expect("game id");
        let dll = game_dir.path().join("nvngx_dlss.dll");
        std::fs::write(&dll, b"catalog-bytes").expect("write DLL");
        let catalog_hash = renderpilot_detection::sha256_file(&dll).expect("catalog hash");
        let component = LibraryComponent::new(
            ComponentId::new("component:dlss-freshness").expect("component id"),
            game_id.clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::DlssSuperResolution,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(PathRef::new(dll.to_string_lossy()).expect("path"))
                .with_sha256(catalog_hash),
        );
        std::fs::write(&dll, b"externally-replaced").expect("replace DLL");

        let refreshed =
            components_for_candidate_matching_with_installed(&game_id, &[component], None)
                .expect("live refresh");

        assert!(
            refreshed.is_empty(),
            "an externally replaced non-OpenVR DLL must not use stale candidate facts",
        );
    }
}
