use std::collections::BTreeMap;

use renderpilot_domain::{GameId, GameInstallation};
use sha2::{Digest, Sha256};

use crate::{
    Context,
    context::{IssuedRetiredLeftoversIntent, RetiredLeftoversObservation},
};

use super::dto::*;
use super::{external_cleanup, local_removal};

mod external;

pub(super) fn build_items(
    context: &Context,
    game: &GameInstallation,
    observation: &RetiredLeftoversObservation,
) -> Vec<LeftoverItem> {
    let mut items = BTreeMap::<String, LeftoverItem>::new();
    for target in local_removal::project_local_targets(observation) {
        insert_item(&mut items, target.item);
    }
    let vulkan_pending = external_cleanup::classify_vulkan_pending(context, observation);

    if let Some(owner) = observation.owners.engine_journal() {
        external::append_engine_item(&mut items, game, owner, observation);
    }
    if let Some(addon) = observation.owners.addon()
        && addon.host_kind() == Some(renderpilot_domain::InstalledAddonHostKind::SharedVulkanLayer)
    {
        external::append_vulkan_item(&mut items, game, addon, observation, vulkan_pending);
    }
    external::append_pending_items(&mut items, game, observation, vulkan_pending);
    items.into_values().collect()
}

pub(super) fn proposal_for(
    game: &GameInstallation,
    root_state: LeftoverRootState,
    semantic_revision: String,
    issued: &IssuedRetiredLeftoversIntent,
    observation: &RetiredLeftoversObservation,
    items: Vec<LeftoverItem>,
) -> LeftoverProposal {
    LeftoverProposal {
        game_id: game.id().as_str().to_owned(),
        game_name: game.identity().title().to_owned(),
        install_path: game.install_path().as_str().to_owned(),
        root_state,
        intent: issued.token.clone(),
        leave_revision: semantic_revision,
        can_clean: observation.pending_nvapi.is_empty()
            && observation.pending_files.is_empty()
            && items.iter().any(|item| {
                matches!(
                    item.disposition,
                    LeftoverDisposition::Cleanable | LeftoverDisposition::Retryable
                )
            }),
        items,
    }
}

pub(super) fn canonical_item_id(
    game_id: &GameId,
    category: LeftoverCategory,
    path: Option<&str>,
    selector: &str,
) -> String {
    let category = serde_json::to_string(&category).unwrap_or_else(|_| "\"unknown\"".to_owned());
    let identity = format!(
        "{}\n{}\n{}\n{}",
        game_id.as_str(),
        category,
        path.unwrap_or_default(),
        selector
    );
    format!("item-{}", hex::encode(Sha256::digest(identity.as_bytes())))
}

pub(super) fn item(
    game_id: &GameId,
    category: LeftoverCategory,
    path: Option<&str>,
    selector: &str,
    disposition: LeftoverDisposition,
    issue_value: Option<LeftoverIssue>,
) -> LeftoverItem {
    LeftoverItem {
        item_id: canonical_item_id(game_id, category, path, selector),
        category,
        path: path.map(str::to_owned),
        disposition,
        issue: issue_value,
    }
}

pub(super) fn insert_item(items: &mut BTreeMap<String, LeftoverItem>, candidate: LeftoverItem) {
    match items.get_mut(&candidate.item_id) {
        None => {
            items.insert(candidate.item_id.clone(), candidate);
        }
        Some(existing) if candidate.disposition == LeftoverDisposition::Blocked => {
            existing.disposition = LeftoverDisposition::Blocked;
            existing.issue = existing.issue.clone().or(candidate.issue);
        }
        Some(_) => {}
    }
}

pub(super) fn issue(code: LeftoverIssueCode, detail: Option<String>) -> LeftoverIssue {
    LeftoverIssue { code, detail }
}
