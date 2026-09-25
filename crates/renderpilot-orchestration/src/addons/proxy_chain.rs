//! Cross-add-on coordination for an OptiScaler/ReShade proxy chain.

mod install;
mod model;
mod publication;
mod relocation;
mod removal;

pub(crate) use model::{DownstreamInstallPlan, ProxyInstallPlan, planned_peer_host_relocation};
pub(crate) use removal::revalidate_for_removal;
#[cfg(test)]
use std::path::Path;

#[cfg(test)]
use renderpilot_domain::{
    FileOwnership, GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef,
    Platform, ProxyImplementation, ProxyLink, ProxyRootPrestate,
};

#[cfg(test)]
use crate::addons::optiscaler::identity::path_ref;
use crate::{Context, ServiceError};
#[cfg(test)]
use renderpilot_application::GameRepository;
#[cfg(test)]
use renderpilot_application::{
    InstalledAddonRepository, OptiScalerStateRepository, ProxyTopologyRepository,
};

#[cfg(test)]
pub(super) fn exact_receipt_from_live(
    path: &Path,
    ownership: FileOwnership,
) -> Result<renderpilot_domain::FileReceipt, ServiceError> {
    publication::exact_receipt_from_live(path, ownership)
}

#[cfg(test)]
pub(super) fn seed_game(context: &Context, game_id: &GameId, root: &Path) {
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "proxy chain test", Launcher::Manual)
            .expect("game identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        path_ref(root).expect("game root path"),
    );
    context.storage().upsert_game(&game).expect("game");
}

#[cfg(test)]
pub(super) fn state_from_config(
    game_id: &GameId,
    root: &Path,
    topology_id: &str,
    config: &Path,
) -> renderpilot_domain::OptiScalerInstallState {
    let installed = exact_receipt_from_live(config, FileOwnership::Owned).expect("config receipt");
    renderpilot_domain::from_new_install(
        renderpilot_domain::OptiScalerInstallStateParts {
            game_id: game_id.clone(),
            release_id: "proxy-chain-test".to_owned(),
            manifest_revision: "proxy-chain-test".to_owned(),
            archive_sha256: None,
            source: None,
            target_exe_path: path_ref(&root.join("Game.exe")).expect("exe path"),
            target_dir: path_ref(root).expect("target path"),
            modules: vec!["core".to_owned()],
            release_files: vec![renderpilot_domain::OptiScalerFileReceipt {
                path: path_ref(config).expect("config path"),
                installed,
                role: renderpilot_domain::OptiScalerFileRole::Configuration,
                cleanup: renderpilot_domain::OptiScalerFileCleanup::RemoveIfUnchanged,
                baseline: renderpilot_domain::OptiScalerReleaseFileBaseline::Absent,
            }],
            runtime_bindings: Vec::new(),
            directory_receipts: Vec::new(),
            proxy_topology_id: Some(topology_id.to_owned()),
            config_schema: 1,
            config_base_release: "proxy-chain-test".to_owned(),
            adoption_state: renderpilot_domain::OptiScalerAdoptionState::Managed,
            prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
            created_at: None,
            updated_at: None,
        },
        renderpilot_domain::OptiScalerConfigurationBaseline::absent(),
    )
    .expect("OptiScaler state")
}

#[cfg(test)]
pub(super) fn commit_journal_aggregate(
    mutation: &mut PreparedFileMutation<'_>,
    context: &Context,
    game_id: &GameId,
    after_state: Option<&renderpilot_domain::OptiScalerInstallState>,
    after_topology: Option<&GameProxyTopology>,
) -> Result<(), ServiceError> {
    let before_state = context
        .storage()
        .get_optiscaler_install_state(game_id)
        .map_err(ServiceError::from)?;
    let before_topology = context
        .storage()
        .get_proxy_topology(game_id)
        .map_err(ServiceError::from)?;
    let before = renderpilot_storage_sqlite::AggregateBefore::new(
        game_id.clone(),
        before_state.clone(),
        before_topology.clone(),
        context
            .storage()
            .get_installed_addon(game_id)
            .map_err(ServiceError::from)?,
    )
    .map_err(ServiceError::from)?;
    let mutation_id = mutation.id().to_owned();
    mutation.commit_journal_aggregate(
        before,
        renderpilot_storage_sqlite::OptiScalerAggregateMutation::FilesystemWithAuxiliary {
            before_state: before_state.as_ref(),
            after_state,
            before_topology: before_topology.as_ref(),
            after_topology,
            peer: renderpilot_storage_sqlite::OptiScalerPeerMutation::Keep,
            auxiliary_preservations: &[],
            retained_claims: &[],
            mutation_id: &mutation_id,
        },
    )
}

#[cfg(test)]
#[path = "proxy_chain/tests/adoption.rs"]
mod focused_test_adoption;
#[cfg(test)]
#[path = "proxy_chain/tests/drift.rs"]
mod focused_test_drift;
#[cfg(test)]
#[path = "proxy_chain/tests/fresh.rs"]
mod focused_test_fresh;
#[cfg(test)]
#[path = "proxy_chain/tests/release.rs"]
mod focused_test_release;

/// Executes a prevalidated outer/downstream topology transition.
///
/// Callers invoke this only inside a prepared durable filesystem mutation.
pub(crate) fn execute_install_plan(
    context: &Context,
    plan: &ProxyInstallPlan<'_>,
    outer_bytes: &[u8],
    mutation: &mut PreparedFileMutation<'_>,
    changed: &mut Vec<String>,
) -> Result<GameProxyTopology, ServiceError> {
    if plan.updating {
        relocation::execute_update(context, plan, outer_bytes, mutation, changed)
    } else {
        install::execute_fresh_install(plan, outer_bytes, mutation, changed)
    }
}

use crate::file_mutation::optiscaler::PreparedFileMutation;
use renderpilot_domain::GameProxyTopology;
