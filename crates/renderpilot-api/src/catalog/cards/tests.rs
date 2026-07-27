use super::bootstrap_filters::parse_bootstrap_filters;
use super::normalize::{expand_library_filter_aliases, normalize_library_name};
use super::output::GameCardOutput;
use super::query::QueryGameCards;
use super::{QueryGameCardsRequest, bootstrap_games_catalog};
use renderpilot_orchestration::catalog::CatalogCardRiskLevel;
use renderpilot_orchestration::catalog::GameCardData;
use renderpilot_orchestration::domain::{
    AddonKind, GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};

fn stub_card(launcher: &str, library_tags: &[&str]) -> GameCardOutput {
    GameCardOutput {
        game_id: String::from("test-id"),
        title: String::from("Test Game"),
        title_search_key: String::from("test game"),
        launcher: String::from(launcher),
        platform: String::from("windows"),
        runtime: String::from("dx11"),
        install_path: String::from("/games/test"),
        external_id: None,
        library_tags: library_tags.iter().map(|&t| String::from(t)).collect(),
        component_count: 1,
        addon_capabilities: Vec::new(),
        updates_available: false,
        update_count: 0,
        risk_level: String::from("safe"),
        risk_order: CatalogCardRiskLevel::Low,
        rollback_available: false,
        operation_count: 0,
        last_operation_status: None,
        cover_updated_at_ms: None,
        is_favorite: false,
        is_hidden: false,
    }
}

#[test]
fn empty_bootstrap_returns_typed_filters_and_catalog_result() {
    let root = tempfile::tempdir().expect("temp catalog");
    let context = renderpilot_orchestration::Context::open_at(root.path().join("catalog.sqlite"))
        .expect("context");

    let output = bootstrap_games_catalog(&context, 120).expect("bootstrap");

    assert_eq!(output["result"]["catalogSize"], 0);
    assert!(
        output["filters"]["libraries"]
            .as_array()
            .is_some_and(|filters| filters.contains(&serde_json::json!("dlss_super_resolution")))
    );
    assert_eq!(
        output["filters"]["addons"],
        serde_json::json!(["renodx", "luma"])
    );
    assert!(output.get("catalogRevision").is_none());
    assert!(output.get("syncState").is_none());
}

#[test]
fn normalize_library_name_keeps_current_slugs_and_drops_unknown() {
    assert_eq!(
        normalize_library_name(" dlss_super_resolution "),
        Some(String::from("dlss_super_resolution")),
    );
    assert_eq!(normalize_library_name("unknown"), None);
    assert_eq!(normalize_library_name("   "), None);
}

#[test]
fn normalize_library_name_rejects_legacy_and_non_slug_values() {
    assert_eq!(normalize_library_name("IntelXeLl"), None);
    assert_eq!(normalize_library_name("steam"), None);
}

#[test]
fn persisted_amd_fsr_filter_expands_to_the_same_query_tags_as_the_frontend() {
    assert_eq!(
        expand_library_filter_aliases(vec![String::from("amd_fsr")]),
        vec![
            String::from("amd_fsr"),
            String::from("amd_fsr_upscaler"),
            String::from("amd_fsr_loader"),
            String::from("amd_fsr_radiance_cache"),
        ],
    );
}

#[test]
fn initial_bootstrap_selection_hides_cards_without_a_selected_technology_or_addon() {
    let persisted = parse_bootstrap_filters(None);
    let selected_libraries = expand_library_filter_aliases(persisted.libraries);
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: persisted.search_query,
            selected_libraries,
            selected_addons: persisted.addons,
            selected_launchers: persisted.launchers,
            launcher_order: persisted.launcher_order,
            show_hidden: persisted.show_hidden,
            favorites_only: persisted.favorites_only,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 120,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let technology_card = stub_card("Steam", &["dlss_super_resolution"]);
    let plain_card = stub_card("Steam", &[]);

    assert!(query.matches(&technology_card));
    assert!(!query.matches(&plain_card));
}

#[test]
fn empty_selected_launchers_matches_all_cards() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: Vec::new(),
            selected_launchers: Vec::new(),
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let steam_card = stub_card("Steam", &["steam"]);
    let epic_card = stub_card("Epic", &["epic"]);

    assert!(query.matches(&steam_card));
    assert!(query.matches(&epic_card));
}

#[test]
fn selected_launcher_matches_only_same_launcher() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: Vec::new(),
            selected_launchers: vec![String::from("Steam")],
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let steam_card = stub_card("Steam", &["steam"]);
    let epic_card = stub_card("Epic", &["epic"]);

    assert!(query.matches(&steam_card));
    assert!(!query.matches(&epic_card));
}

#[test]
fn known_launcher_absent_from_current_facets_remains_an_active_filter() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: Vec::new(),
            selected_launchers: vec![String::from("Epic")],
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let steam_card = stub_card("Steam", &["steam"]);

    assert!(!query.matches(&steam_card));
}

#[test]
fn unknown_selected_launcher_is_ignored() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: Vec::new(),
            selected_launchers: vec![String::from("StaleLauncher")],
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let steam_card = stub_card("Steam", &["steam"]);

    assert!(query.matches(&steam_card));
}

#[test]
fn empty_selected_libraries_matches_all_cards() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: Vec::new(),
            selected_launchers: Vec::new(),
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let steam_card = stub_card("Steam", &["dlss_super_resolution"]);

    assert!(query.matches(&steam_card));
}

#[test]
fn selected_library_matches_only_same_library() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: vec![String::from("dlss_super_resolution")],
            selected_addons: Vec::new(),
            selected_launchers: Vec::new(),
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let dlss_card = stub_card("Steam", &["dlss_super_resolution"]);
    let xess_card = stub_card("Epic", &["intel_xess"]);

    assert!(query.matches(&dlss_card));
    assert!(!query.matches(&xess_card));
}

#[test]
fn known_library_absent_from_current_facets_remains_an_active_filter() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: vec![String::from("intel_xess")],
            selected_addons: Vec::new(),
            selected_launchers: Vec::new(),
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let dlss_card = stub_card("Steam", &["dlss_super_resolution"]);

    assert!(!query.matches(&dlss_card));
}

#[test]
fn unknown_selected_library_is_ignored() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: vec![String::from("stale-library")],
            selected_addons: Vec::new(),
            selected_launchers: Vec::new(),
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let dlss_card = stub_card("Steam", &["dlss_super_resolution"]);

    assert!(query.matches(&dlss_card));
}

#[test]
fn no_filters_keep_a_card_without_components_or_addons() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: Vec::new(),
            selected_launchers: Vec::new(),
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let mut card = stub_card("Steam", &[]);
    card.component_count = 0;

    assert!(query.matches(&card));
}

#[test]
fn selected_addon_matches_only_cards_with_that_capability() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: vec![String::from("luma")],
            selected_launchers: Vec::new(),
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let mut luma_card = stub_card("Steam", &[]);
    luma_card.addon_capabilities = vec![renderpilot_orchestration::domain::AddonKind::Luma];
    let plain_card = stub_card("Steam", &[]);

    assert!(query.matches(&luma_card));
    assert!(!query.matches(&plain_card));
}

#[test]
fn unknown_selected_addons_do_not_empty_the_catalog() {
    // Unknown tokens are dropped by normalize; an all-unknown selection must
    // become "no addon filter", not "filter active + empty match set".
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: vec![String::from("unknown")],
            selected_launchers: Vec::new(),
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let mut card = stub_card("Steam", &[]);
    card.addon_capabilities = vec![renderpilot_orchestration::domain::AddonKind::Luma];
    let plain = stub_card("Steam", &[]);

    assert!(query.matches(&card));
    assert!(query.matches(&plain));
}

#[test]
fn mixed_known_and_unknown_selected_addons_filter_by_known_only() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: vec![String::from("luma"), String::from("unknown")],
            selected_launchers: Vec::new(),
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let mut luma_card = stub_card("Steam", &[]);
    luma_card.addon_capabilities = vec![renderpilot_orchestration::domain::AddonKind::Luma];
    let plain = stub_card("Steam", &[]);

    assert!(query.matches(&luma_card));
    assert!(!query.matches(&plain));
}

#[test]
fn library_and_addon_filters_are_combined_with_or() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: vec![String::from("dlss_super_resolution")],
            selected_addons: vec![String::from("luma")],
            selected_launchers: Vec::new(),
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let mut both = stub_card("Steam", &["dlss_super_resolution"]);
    both.addon_capabilities = vec![renderpilot_orchestration::domain::AddonKind::Luma];
    let library_only = stub_card("Steam", &["dlss_super_resolution"]);
    let mut addon_only = stub_card("Steam", &[]);
    addon_only.addon_capabilities = vec![renderpilot_orchestration::domain::AddonKind::Luma];

    assert!(query.matches(&both));
    assert!(query.matches(&library_only));
    assert!(query.matches(&addon_only));
}

#[test]
fn visual_order_is_launcher_then_favorite_then_sort_then_stable_id() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: Vec::new(),
            selected_launchers: Vec::new(),
            launcher_order: vec![String::from("Epic"), String::from("Steam")],
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("desc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Epic"), String::from("Steam")],
    );
    let mut epic = stub_card("Epic", &[]);
    epic.game_id = String::from("epic");
    epic.title = String::from("A");
    let mut steam_favorite = stub_card("Steam", &[]);
    steam_favorite.game_id = String::from("favorite");
    steam_favorite.title = String::from("A");
    steam_favorite.is_favorite = true;
    let mut steam_plain = stub_card("Steam", &[]);
    steam_plain.game_id = String::from("plain");
    steam_plain.title = String::from("Z");
    let mut cards = [steam_plain, steam_favorite, epic];

    cards.sort_by(|left, right| query.compare(left, right));

    assert_eq!(
        cards
            .iter()
            .map(|card| card.game_id.as_str())
            .collect::<Vec<_>>(),
        vec!["epic", "favorite", "plain"]
    );
}

#[test]
fn stable_game_id_is_not_reversed_by_descending_sort() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: Vec::new(),
            selected_launchers: Vec::new(),
            launcher_order: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("desc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("Steam")],
    );
    let mut left = stub_card("Steam", &[]);
    left.game_id = String::from("a");
    let mut right = stub_card("Steam", &[]);
    right.game_id = String::from("b");
    let mut cards = [right, left];

    cards.sort_by(|left, right| query.compare(left, right));

    assert_eq!(cards[0].game_id, "a");
    assert_eq!(cards[1].game_id, "b");
}

#[test]
fn direct_card_facts_match_the_legacy_dto_query_for_all_semantic_axes() {
    let cards = [
        data_card(
            "a",
            "Alpha",
            Launcher::Steam,
            &["dlss_super_resolution"],
            &[],
            true,
            false,
            2,
            CatalogCardRiskLevel::Low,
        ),
        data_card(
            "b",
            "Beta",
            Launcher::Epic,
            &["intel_xess"],
            &[AddonKind::Luma],
            false,
            false,
            1,
            CatalogCardRiskLevel::High,
        ),
        data_card(
            "c",
            "Gamma",
            Launcher::Steam,
            &["fsr_upscaler"],
            &[AddonKind::RenoDx],
            false,
            true,
            0,
            CatalogCardRiskLevel::Medium,
        ),
    ];
    let legacy = cards
        .iter()
        .map(GameCardOutput::from_card)
        .collect::<Vec<_>>();
    let filters = [
        ("", vec![], vec![], vec![], false, false),
        (
            "a",
            vec!["dlss_super_resolution"],
            vec![],
            vec!["Steam"],
            false,
            false,
        ),
        ("", vec!["intel_xess"], vec!["luma"], vec![], true, false),
        ("", vec![], vec![], vec![], true, true),
    ];

    for sort_field in ["title", "updates", "risk"] {
        for sort_direction in ["asc", "desc"] {
            for launcher_order in [vec!["Steam", "Epic"], vec!["Epic", "Steam"]] {
                for (search, libraries, addons, launchers, show_hidden, favorites_only) in &filters
                {
                    for (page_limit, page_offset) in [(1, 0), (2, 1), (120, 0)] {
                        let query = QueryGameCards::new(
                            QueryGameCardsRequest {
                                search_query: (*search).to_owned(),
                                selected_libraries: strings(libraries),
                                selected_addons: strings(addons),
                                selected_launchers: strings(launchers),
                                launcher_order: strings(&launcher_order),
                                show_hidden: *show_hidden,
                                favorites_only: *favorites_only,
                                sort_field: sort_field.to_owned(),
                                sort_direction: sort_direction.to_owned(),
                                page_limit,
                                page_offset,
                            },
                            &["Epic".to_owned(), "Steam".to_owned()],
                        );

                        let mut direct = cards
                            .iter()
                            .filter(|card| query.matches(*card))
                            .collect::<Vec<_>>();
                        direct.sort_by(|left, right| query.compare(*left, *right));
                        let mut dto = legacy
                            .iter()
                            .filter(|card| query.matches(*card))
                            .collect::<Vec<_>>();
                        dto.sort_by(|left, right| query.compare(*left, *right));

                        let direct_page = direct[query.page.bounds(direct.len())]
                            .iter()
                            .map(|card| card.game.id().as_str())
                            .collect::<Vec<_>>();
                        let dto_page = dto[query.page.bounds(dto.len())]
                            .iter()
                            .map(|card| card.game_id.as_str())
                            .collect::<Vec<_>>();
                        assert_eq!(direct_page, dto_page);
                    }
                }
            }
        }
    }
}

#[test]
fn generated_catalogs_keep_stable_page_bounds_at_supported_scales() {
    for catalog_size in [10, 1_000, 10_000] {
        let mut cards = (0..catalog_size)
            .map(|index| {
                let mut card = stub_card(
                    if index % 2 == 0 { "Steam" } else { "Epic" },
                    if index % 3 == 0 {
                        &["dlss_super_resolution"]
                    } else {
                        &["intel_xess"]
                    },
                );
                card.game_id = format!("generated:{index:05}");
                card.title = format!("Generated Game {index:05}");
                card.title_search_key = card.title.to_lowercase();
                card
            })
            .collect::<Vec<_>>();
        let query = QueryGameCards::new(
            QueryGameCardsRequest {
                search_query: "generated".to_owned(),
                selected_libraries: vec!["dlss_super_resolution".to_owned()],
                selected_addons: Vec::new(),
                selected_launchers: Vec::new(),
                launcher_order: vec!["Steam".to_owned(), "Epic".to_owned()],
                show_hidden: false,
                favorites_only: false,
                sort_field: "title".to_owned(),
                sort_direction: "asc".to_owned(),
                page_limit: 120,
                page_offset: 0,
            },
            &["Epic".to_owned(), "Steam".to_owned()],
        );
        cards.retain(|card| query.matches(card));
        cards.sort_by(|left, right| query.compare(left, right));

        let page = &cards[query.page.bounds(cards.len())];
        assert_eq!(page.len(), cards.len().min(120));
        assert!(
            page.windows(2)
                .all(|pair| { query.compare(&pair[0], &pair[1]) != std::cmp::Ordering::Greater })
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn data_card(
    id: &str,
    title: &str,
    launcher: Launcher,
    library_tags: &[&str],
    addon_capabilities: &[AddonKind],
    is_favorite: bool,
    is_hidden: bool,
    update_count: usize,
    risk_level: CatalogCardRiskLevel,
) -> GameCardData {
    let game_id = GameId::new(format!("golden:{id}")).expect("game id");
    let identity = GameIdentity::new(game_id, title, launcher).expect("identity");
    GameCardData {
        game: GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(format!("C:/Games/{id}")).expect("path"),
        ),
        title_search_key: title.to_lowercase(),
        library_tags: library_tags
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        component_count: library_tags.len(),
        update_count,
        risk_level,
        cover_updated_at_ms: None,
        rollback_available: false,
        operation_count: 0,
        last_operation_status: None,
        is_favorite,
        is_hidden,
        addon_capabilities: addon_capabilities.to_vec(),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}
