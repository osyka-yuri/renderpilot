//! Input normalization for the game-card query: search text, page bounds, and
//! domain-valid library / launcher filter values.

use renderpilot_orchestration::domain::{AddonKind, Launcher, LibraryTechnology};

const AMD_FSR_FILTER_ALIAS_MEMBERS: &[LibraryTechnology] = &[
    LibraryTechnology::AmdFsrUpscaler,
    LibraryTechnology::AmdFsrLoader,
    LibraryTechnology::AmdFsrRadianceCache,
];

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

pub(super) fn expand_library_filter_aliases(values: Vec<String>) -> Vec<String> {
    let mut expanded = Vec::with_capacity(values.len());

    for value in values {
        let is_amd_fsr_alias =
            LibraryTechnology::from_slug(&value) == Some(LibraryTechnology::AmdFsr);
        expanded.push(value);

        if is_amd_fsr_alias {
            expanded.extend(
                AMD_FSR_FILTER_ALIAS_MEMBERS
                    .iter()
                    .map(|technology| technology.as_slug().to_owned()),
            );
        }
    }

    expanded
}

pub(super) fn normalize_library_name(value: &str) -> Option<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return None;
    }

    match parse_library_technology(trimmed) {
        Some(LibraryTechnology::Unknown) => None,
        Some(technology) => Some(technology.as_slug().to_owned()),
        None => None,
    }
}

pub(super) fn normalize_selected_libraries(selected_libraries: Vec<String>) -> Vec<String> {
    normalize_library_names(selected_libraries)
}

pub(super) fn normalize_addon_names(values: Vec<String>) -> Vec<String> {
    let mut normalized = values
        .into_iter()
        .filter_map(|value| AddonKind::from_stable_str(&value).map(|kind| kind.as_str().to_owned()))
        .collect::<Vec<_>>();

    normalized.sort();
    normalized.dedup();
    normalized
}

pub(super) fn normalize_launcher_names(values: Vec<String>) -> Vec<String> {
    let mut normalized = values
        .into_iter()
        .filter_map(|value| {
            Launcher::from_stable_str(value.trim()).map(|launcher| launcher.as_str().to_owned())
        })
        .collect::<Vec<_>>();

    normalized.sort();
    normalized.dedup();
    normalized
}

pub(super) fn normalize_selected_launchers(selected_launchers: Vec<String>) -> Vec<String> {
    normalize_launcher_names(selected_launchers)
}

fn parse_library_technology(value: &str) -> Option<LibraryTechnology> {
    LibraryTechnology::from_slug(value)
}
