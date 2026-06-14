use super::normalize::normalize_library_name;
use super::output::GameCardOutput;
use super::query::QueryGameCards;
use super::QueryGameCardsRequest;
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
