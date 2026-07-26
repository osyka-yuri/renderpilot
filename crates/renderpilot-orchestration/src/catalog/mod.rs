//! Catalog orchestration: scan, query, and library management.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use renderpilot_application::{
    AppError, AppResult, ArtifactRepository, ComponentReplacementCandidates, ComponentRepository,
    GameRepository, InstalledAddonRepository, OperationPlan, OperationRecord,
    find_replacement_candidates,
};
use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::{
    AddonKind, GameId, GameInstallation, GraphicsComponent, GraphicsTechnology, LibraryArtifact,
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

use self::scan::scan_folder_impl;

#[cfg(windows)]
pub mod auto_scan;
mod cards;
pub(crate) mod cascade;
pub mod execute;
mod operations;
pub mod output;
mod runtime_compatibility;
/// Auto-discovery and scanning.
pub mod scan;
mod source_assessment;
mod swap;

#[cfg(windows)]
pub use scan::prune_auto_scan_orphans;

/// The game installation and detected library files produced by a folder scan.
pub struct ScanFolderCatalogResult {
    /// The game installation discovered at the scanned path.
    pub game: GameInstallation,
    /// Library files detected within the game installation.
    pub libraries: Vec<DetectedLibraryFile>,
}

/// Game id and replacement candidate groups for a component swap UI.
pub struct CandidateCatalogResult {
    /// The game id the candidates belong to.
    pub game_id: GameId,
    /// Grouped replacement candidates, one group per component.
    pub groups: Vec<ComponentReplacementCandidates>,
}

/// Full game details for the main detail view.
pub struct GameDetailsCatalogResult {
    /// The game installation.
    pub game: GameInstallation,
    /// All graphics components for this game.
    pub components: Vec<GraphicsComponent>,
    /// Replacement candidate groups across all components.
    pub candidate_groups: Vec<ComponentReplacementCandidates>,
    /// Fresh active D3D12 executable status, independent of candidate availability.
    pub d3d12_executable_status: Option<D3d12ExecutableStatus>,
    /// Operation history for this game.
    pub operations: OperationListCatalogResult,
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
pub struct OperationListCatalogResult {
    /// The game id the operations belong to.
    pub game_id: GameId,
    /// Ordered list of operation entries.
    pub operations: Vec<OperationListCatalogEntry>,
}

/// A single entry in the operation history list.
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
    let profile = context
        .profile_capability_snapshot()
        .capabilities_for(game_id);
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

/// Scans a folder path for game installations and persists or updates catalog rows.
pub fn scan_folder(
    context: &crate::Context,
    path: PathBuf,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    scan_folder_impl(context, path)
}

/// Returns all game installations stored in the catalog.
pub fn list_games(context: &crate::Context) -> Result<Vec<GameInstallation>, ServiceError> {
    context.storage().list_games().map_err(Into::into)
}

/// The per-query-constant inputs for replacement-candidate matching: the local +
/// catalog artifact universe and the candidate context.
///
/// Loading these is independent of the game, so a multi-game caller (the
/// dashboard's [`game_cards`]) builds this **once** and reuses it for every game
/// instead of re-reading the artifacts table and catalog snapshot per game.
pub(crate) struct ReplacementUniverse {
    artifacts: Vec<LibraryArtifact>,
    candidate_context: renderpilot_application::CandidateContext,
}

/// Loads the artifact universe (local artifacts + catalog packages) and the
/// candidate context once. A missing or invalid catalog degrades to local-only
/// artifacts.
pub(crate) fn load_replacement_universe(
    context: &crate::Context,
) -> Result<ReplacementUniverse, ServiceError> {
    let local_artifacts = context.storage().list_artifacts()?;

    let downloaded_ids: HashSet<_> = local_artifacts.iter().map(|a| a.id().clone()).collect();
    let mut artifacts = local_artifacts;
    let (catalog_package_ids, debug_package_ids) =
        match crate::libraries::catalog_packages_as_artifacts() {
            Ok(catalog_artifacts) => {
                let (catalog_artifacts, package_ids, debug_package_ids) =
                    catalog_artifacts.into_parts();
                artifacts.extend(
                    catalog_artifacts
                        .into_iter()
                        .filter(|a| !downloaded_ids.contains(a.id())),
                );
                (package_ids, debug_package_ids)
            }
            Err(error) => {
                log::warn!("could not load catalog replacement artifacts: {error}");
                (HashMap::new(), HashSet::new())
            }
        };

    let candidate_context = renderpilot_application::CandidateContext::new(
        downloaded_ids,
        catalog_package_ids,
        debug_package_ids,
    );

    Ok(ReplacementUniverse {
        artifacts,
        candidate_context,
    })
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

    let d3d12_component = components
        .iter()
        .find(|component| component.technology() == GraphicsTechnology::D3D12Agility);
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
        .clone()
        .with_target_profile(target_profile);
    let matching_components = components_for_candidate_matching(context, game_id, &components)?;
    let candidate_groups = find_replacement_candidates(
        &matching_components,
        &universe.artifacts,
        &candidate_context,
    );

    let operations = list_operations(context, game_id)?;

    Ok(GameDetailsCatalogResult {
        game,
        components,
        candidate_groups,
        d3d12_executable_status,
        operations,
    })
}

fn build_d3d12_status(
    component: &GraphicsComponent,
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
pub(crate) fn components_for_candidate_matching(
    context: &crate::Context,
    game_id: &GameId,
    components: &[GraphicsComponent],
) -> Result<Vec<GraphicsComponent>, ServiceError> {
    if !components
        .iter()
        .any(|component| component.technology() == GraphicsTechnology::OpenVr)
    {
        return Ok(components.to_vec());
    }

    let installed_addon = context.storage().get_installed_addon(game_id)?;
    let managed_files = crate::coordinated_files::managed_files_of(installed_addon.as_ref());

    Ok(components
        .iter()
        .filter_map(|component| {
            if component.technology() != GraphicsTechnology::OpenVr {
                return Some(component.clone());
            }
            match crate::coordinated_files::current_component_snapshot(component, managed_files) {
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
        .collect())
}

/// Returns full game details including components, candidates, and operations.
pub fn get_game_details(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<GameDetailsCatalogResult, ServiceError> {
    let universe = load_replacement_universe(context)?;
    get_game_details_with_universe(context, game_id, &universe)
}

/// Returns library artifacts stored in the catalog, optionally filtered by technology.
pub fn list_artifacts(
    context: &crate::Context,
    technology: Option<GraphicsTechnology>,
) -> Result<Vec<LibraryArtifact>, ServiceError> {
    let artifacts = context.storage().list_artifacts()?;
    Ok(filter_artifacts_by_technology(artifacts, technology))
}

// Re-export core operations from sub-modules directly.
pub use cards::{GameCardData, game_cards};
pub use execute::{
    D3d12ExecutableActionResult, D3d12ExecutableActionResultKind, RollbackPlan, apply_swap,
    apply_swap_confirmed, build_rollback_plan, rollback_component,
};
pub use operations::list_operations;
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
    let current = storage.get_game_ui_state(game_id.as_str())?;
    let (prev_favorite, prev_hidden) = current
        .map(|state| (state.is_favorite, state.is_hidden))
        .unwrap_or((false, false));
    let (is_favorite, is_hidden) = f(prev_favorite, prev_hidden);
    storage
        .save_game_ui_state(game_id.as_str(), is_favorite, is_hidden)
        .map_err(Into::into)
}

fn filter_artifacts_by_technology(
    artifacts: Vec<LibraryArtifact>,
    technology: Option<GraphicsTechnology>,
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
        let component = GraphicsComponent::new(
            ComponentId::new("component:openvr-freshness").expect("component id"),
            game_id.clone(),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::OpenVr,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(PathRef::new(dll.to_string_lossy()).expect("path"))
                .with_sha256(hash)
                .with_pe_compatibility(profile),
        );

        let refreshed =
            components_for_candidate_matching(&context, &game_id, &[component]).expect("refresh");
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
            GraphicsTechnology::OpenVr,
            renderpilot_domain::openvr::DLL_NAME,
            vec![candidate_file],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
        .with_metadata(metadata);

        assert!(
            find_replacement_candidates(
                &refreshed,
                &[candidate],
                &renderpilot_application::CandidateContext::empty(),
            )
            .is_empty(),
            "an OpenVR DLL without a freshly observed profile must have no dropdown candidates"
        );
    }
}
