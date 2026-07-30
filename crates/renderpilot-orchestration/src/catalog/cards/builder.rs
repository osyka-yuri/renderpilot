use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use renderpilot_application::{InstalledAddonRepository, find_replacement_candidates_indexed};
use renderpilot_domain::{GameId, InstalledAddon, LibraryComponent, LibraryTechnology};

use crate::ServiceError;
use crate::catalog::{load_replacement_universe, merge_addon_capabilities};

use super::facts::{SnapshotFactsMode, card_dynamic_facts, card_metrics};
use super::index::build_string_index;
use super::{CatalogRevision, CatalogSnapshot, GameCardData};

pub(super) fn build_snapshot(
    context: &crate::Context,
    revision: CatalogRevision,
    facts_mode: SnapshotFactsMode,
) -> Result<CatalogSnapshot, ServiceError> {
    let storage = context.storage();
    let games = storage.list_games()?;

    let mut components_by_game = HashMap::<GameId, Vec<LibraryComponent>>::new();
    for component in storage.list_all_components()? {
        components_by_game
            .entry(component.game_id().clone())
            .or_default()
            .push(component);
    }

    let covers_by_game = storage.list_all_game_covers()?;
    let ui_states: HashMap<String, _> = storage
        .list_all_game_ui_state()?
        .into_iter()
        .map(|mut row| {
            let game_id = std::mem::take(&mut row.game_id);
            (game_id, row)
        })
        .collect();
    let mut component_backups = storage.list_all_component_backups()?;
    let executable_overrides = if facts_mode == SnapshotFactsMode::ValidateLive {
        storage
            .list_nvapi_executable_overrides()?
            .into_iter()
            .map(|row| (row.game_id, PathBuf::from(row.selected_path)))
            .collect::<HashMap<_, _>>()
    } else {
        HashMap::new()
    };

    let mut operations_by_game = HashMap::<GameId, (usize, Option<(i64, String)>)>::new();
    for operation in storage.list_all_operation_headers()? {
        let created_at = operation.created_at.as_i64();
        let summary = operations_by_game
            .entry(operation.game_id)
            .or_insert((0, None));
        summary.0 += 1;
        if summary
            .1
            .as_ref()
            .is_none_or(|(latest, _)| created_at > *latest)
        {
            summary.1 = Some((created_at, operation.status.as_str().to_owned()));
        }
    }

    let installed_records: HashMap<GameId, InstalledAddon> = storage
        .list_installed_addons()?
        .into_iter()
        .filter(crate::addons::tool::record_is_active)
        .map(|addon| (addon.game_id().clone(), addon))
        .collect();
    let profile_capabilities =
        crate::addons::capabilities::DurableProfileCapabilities::load(context)?;
    let universe = load_replacement_universe(context)?;

    let mut cards = Vec::with_capacity(games.len());
    let mut libraries = BTreeSet::new();
    let mut launchers = BTreeSet::new();

    for game in games {
        let components = components_by_game.remove(game.id()).unwrap_or_default();
        let dynamic_facts = card_dynamic_facts(
            facts_mode,
            &game,
            &components,
            installed_records.get(game.id()),
            &mut component_backups,
            executable_overrides
                .get(game.id().as_str())
                .map(PathBuf::as_path),
        )?;
        let candidate_context = universe
            .candidate_context
            .with_target_profile(dynamic_facts.target_profile);
        let candidate_groups = find_replacement_candidates_indexed(
            &dynamic_facts.matching_components,
            &universe.artifact_index,
            &candidate_context,
        );

        let metrics = card_metrics(&components, &candidate_groups);

        libraries.extend(
            components
                .iter()
                .filter(|component| component.technology() != LibraryTechnology::Unknown)
                .map(|component| component.technology().as_slug().to_owned()),
        );
        launchers.insert(game.identity().launcher().as_str().to_owned());

        let ui_state = ui_states.get(game.id().as_str());
        let profile = profile_capabilities.capabilities_for(game.id());
        let installed_kind = installed_records.get(game.id()).map(InstalledAddon::kind);
        let (operation_count, last_operation_status) = operations_by_game
            .remove(game.id())
            .map(|(count, latest)| (count, latest.map(|(_, status)| status)))
            .unwrap_or_default();

        cards.push(Arc::new(GameCardData {
            cover_updated_at_ms: covers_by_game
                .get(game.id())
                .map(|record| record.updated_at_ms),
            rollback_available: dynamic_facts.rollback_available,
            operation_count,
            last_operation_status,
            is_favorite: ui_state.is_some_and(|state| state.is_favorite),
            is_hidden: ui_state.is_some_and(|state| state.is_hidden),
            addon_capabilities: merge_addon_capabilities(&profile, installed_kind),
            title_search_key: game.identity().title().to_lowercase(),
            library_tags: metrics.library_tags,
            component_count: metrics.component_count,
            update_count: metrics.update_count,
            risk_level: metrics.risk_level,
            game,
        }));
    }

    let card_index = cards
        .iter()
        .enumerate()
        .map(|(index, card)| (card.game.id().clone(), index))
        .collect();
    let launcher_index = build_string_index(&cards, |card| {
        std::iter::once(card.game.identity().launcher().as_str())
    });
    let library_index =
        build_string_index(&cards, |card| card.library_tags.iter().map(String::as_str));
    let addon_index = build_string_index(&cards, |card| {
        card.addon_capabilities.iter().map(|kind| kind.as_str())
    });
    let hidden_count = cards.iter().filter(|card| card.is_hidden).count();

    Ok(CatalogSnapshot {
        revision,
        cards: cards.into(),
        available_libraries: libraries.into_iter().collect::<Vec<_>>().into(),
        available_launchers: launchers.into_iter().collect::<Vec<_>>().into(),
        card_index: Arc::new(card_index),
        launcher_index: Arc::new(launcher_index),
        library_index: Arc::new(library_index),
        addon_index: Arc::new(addon_index),
        hidden_count,
    })
}
