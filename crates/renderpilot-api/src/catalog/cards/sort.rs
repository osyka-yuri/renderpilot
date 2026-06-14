//! Sort fields, directions, and page slicing for the game-card query.

use serde::Serialize;
use std::cmp::Ordering;

use super::normalize::{normalize_page_limit, normalize_page_offset};
use super::output::GameCardOutput;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueryGameCardsSort {
    pub(super) field: QuerySortField,
    pub(super) direction: QuerySortDirection,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueryGameCardsPage {
    limit: usize,
    offset: usize,
}

impl QueryGameCardsSort {
    pub(super) fn new(field: &str, direction: &str) -> Self {
        Self {
            field: QuerySortField::from_input(field),
            direction: QuerySortDirection::from_input(direction),
        }
    }
}

impl QueryGameCardsPage {
    pub(super) fn new(limit: i64, offset: i64) -> Self {
        Self {
            limit: normalize_page_limit(limit),
            offset: normalize_page_offset(offset),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum QuerySortField {
    Title,
    Updates,
    Risk,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum QuerySortDirection {
    Asc,
    Desc,
}

impl QuerySortField {
    fn from_input(value: &str) -> Self {
        match value.trim() {
            value if value.eq_ignore_ascii_case("updates") => Self::Updates,
            value if value.eq_ignore_ascii_case("risk") => Self::Risk,
            _ => Self::Title,
        }
    }
}

impl QuerySortDirection {
    fn from_input(value: &str) -> Self {
        if value.trim().eq_ignore_ascii_case("desc") {
            Self::Desc
        } else {
            Self::Asc
        }
    }

    pub(super) fn apply(self, ordering: Ordering) -> Ordering {
        match self {
            Self::Asc => ordering,
            Self::Desc => ordering.reverse(),
        }
    }
}

pub(super) fn compare_game_card_identity(
    left: &GameCardOutput,
    right: &GameCardOutput,
) -> Ordering {
    left.title
        .cmp(&right.title)
        .then_with(|| left.game_id.cmp(&right.game_id))
}

pub(super) fn page_items(
    items: &[GameCardOutput],
    page: QueryGameCardsPage,
) -> Vec<GameCardOutput> {
    let total = items.len();
    let start = page.offset.min(total);
    let end = start.saturating_add(page.limit).min(total);

    items[start..end].to_vec()
}
