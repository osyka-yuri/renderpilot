use super::QueryGameCardsRequest;
use super::normalize::normalize_library_name;
use super::output::GameCardOutput;
use super::query::QueryGameCards;
use crate::utils::DashboardRiskLevel;

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
        risk_order: DashboardRiskLevel::Low,
        rollback_available: false,
        operation_count: 0,
        last_operation_status: None,
        cover_updated_at_ms: None,
        is_favorite: false,
        is_hidden: false,
    }
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
fn empty_selected_launchers_matches_all_cards() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: Vec::new(),
            selected_launchers: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("steam")],
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
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("steam")],
        &[String::from("Steam")],
    );
    let steam_card = stub_card("Steam", &["steam"]);
    let epic_card = stub_card("Epic", &["epic"]);

    assert!(query.matches(&steam_card));
    assert!(!query.matches(&epic_card));
}

#[test]
fn selected_launcher_not_in_available_excludes_all() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: Vec::new(),
            selected_launchers: vec![String::from("Epic")],
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("steam")],
        &[String::from("Steam")],
    );
    let steam_card = stub_card("Steam", &["steam"]);

    assert!(!query.matches(&steam_card));
}

#[test]
fn empty_selected_libraries_matches_all_cards() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: Vec::new(),
            selected_launchers: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[
            String::from("dlss_super_resolution"),
            String::from("intel_xess"),
        ],
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
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[
            String::from("dlss_super_resolution"),
            String::from("intel_xess"),
        ],
        &[String::from("Steam")],
    );
    let dlss_card = stub_card("Steam", &["dlss_super_resolution"]);
    let xess_card = stub_card("Epic", &["intel_xess"]);

    assert!(query.matches(&dlss_card));
    assert!(!query.matches(&xess_card));
}

#[test]
fn selected_library_not_in_available_excludes_all() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: vec![String::from("intel_xess")],
            selected_addons: Vec::new(),
            selected_launchers: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("dlss_super_resolution")],
        &[String::from("Steam")],
    );
    let dlss_card = stub_card("Steam", &["dlss_super_resolution"]);

    assert!(!query.matches(&dlss_card));
}

#[test]
fn no_filters_keep_a_card_without_components_or_addons() {
    let query = QueryGameCards::new(
        QueryGameCardsRequest {
            search_query: String::new(),
            selected_libraries: Vec::new(),
            selected_addons: Vec::new(),
            selected_launchers: Vec::new(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("dlss_super_resolution")],
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
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[],
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
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[],
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
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[],
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
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 100,
            page_offset: 0,
        },
        &[String::from("dlss_super_resolution")],
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
