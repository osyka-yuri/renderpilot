//! Input normalization for the game-card query: search text, page bounds, and
//! the library / launcher filter sets validated against what is available.

use renderpilot_orchestration::domain::GraphicsTechnology;
use std::collections::BTreeSet;

pub(super) fn normalize_search_query(value: &str) -> String {
    value.trim().to_lowercase()
}

pub(super) fn normalize_page_limit(value: i64) -> usize {
    usize::try_from(value.max(1)).unwrap_or(usize::MAX)
}

pub(super) fn normalize_page_offset(value: i64) -> usize {
    usize::try_from(value.max(0)).unwrap_or(usize::MAX)
}

pub(super) fn normalize_library_names(values: Vec<String>) -> Vec<String> {
    let mut normalized = values
        .into_iter()
        .filter_map(|value| normalize_library_name(&value))
        .collect::<Vec<_>>();

    normalized.sort();
    normalized.dedup();
    normalized
}

pub(super) fn normalize_library_name(value: &str) -> Option<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return None;
    }

    match parse_graphics_technology(trimmed) {
        Some(GraphicsTechnology::Unknown) => None,
        Some(technology) => Some(technology.as_slug().to_owned()),
        None => None,
    }
}

pub(super) fn normalize_selected_libraries(
    selected_libraries: Vec<String>,
    available_libraries: &[String],
) -> Vec<String> {
    if available_libraries.is_empty() {
        return Vec::new();
    }

    let allowed_libraries = available_libraries
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    let mut selected_libraries = normalize_library_names(selected_libraries);

    selected_libraries.retain(|library| allowed_libraries.contains(library.as_str()));

    selected_libraries
}

pub(super) fn normalize_launcher_names(values: Vec<String>) -> Vec<String> {
    let mut normalized = values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    normalized.sort();
    normalized.dedup();
    normalized
}

pub(super) fn normalize_selected_launchers(
    selected_launchers: Vec<String>,
    available_launchers: &[String],
) -> Vec<String> {
    if available_launchers.is_empty() {
        return Vec::new();
    }

    let allowed_launchers = available_launchers
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    let mut selected_launchers = normalize_launcher_names(selected_launchers);

    selected_launchers.retain(|launcher| allowed_launchers.contains(launcher.as_str()));

    selected_launchers
}

fn parse_graphics_technology(value: &str) -> Option<GraphicsTechnology> {
    GraphicsTechnology::from_slug(value)
}
