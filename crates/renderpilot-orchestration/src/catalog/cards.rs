//! Immutable catalog-card snapshot built from a fixed set of batch reads.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use renderpilot_domain::{AddonKind, GameId, GameInstallation};

use crate::ServiceError;

mod builder;
mod facts;
mod index;

use self::facts::SnapshotFactsMode;
use self::index::{collect_index_union, intersect_sorted};

/// Monotonic revision of the process-local catalog projection.
pub type CatalogRevision = u64;

/// One dashboard card assembled without constructing a full game-details model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameCardData {
    /// Installation identity and paths shown by the card.
    pub game: GameInstallation,
    /// Lower-cased title used by search without per-query allocation.
    pub title_search_key: String,
    /// Visible graphics technologies shown and filtered by the UI.
    pub library_tags: Vec<String>,
    /// Number of visible graphics components.
    pub component_count: usize,
    /// Number of visible components with an automatic newer candidate.
    pub update_count: usize,
    /// Precomputed dashboard risk used by sorting and DTO mapping.
    pub risk_level: CatalogCardRiskLevel,
    /// Cover cache-busting timestamp.
    pub cover_updated_at_ms: Option<i64>,
    /// Best current rollback availability. The first projection is derived
    /// from durable baselines and is corrected by background live validation.
    pub rollback_available: bool,
    /// Number of operation headers for the game.
    pub operation_count: usize,
    /// Status of the newest operation, when present.
    pub last_operation_status: Option<String>,
    /// Persisted favorite flag.
    pub is_favorite: bool,
    /// Persisted hidden flag.
    pub is_hidden: bool,
    /// Profile-derived and installed add-on capabilities.
    pub addon_capabilities: Vec<AddonKind>,
}

/// Stable, presentation-neutral risk ordering for a catalog card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CatalogCardRiskLevel {
    /// No visible component exposes a known swappability risk.
    Unknown,
    /// All observed risk is in directly swappable components.
    Low,
    /// At least one component is bundle-only or read-only.
    Medium,
    /// At least one component is unsafe or engine-integrated.
    High,
}

impl CatalogCardRiskLevel {
    /// Stable lowercase value used by transport DTOs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Immutable catalog read model shared by every card query.
#[derive(Debug, Clone)]
pub struct CatalogSnapshot {
    revision: CatalogRevision,
    cards: Arc<[Arc<GameCardData>]>,
    available_libraries: Arc<[String]>,
    available_launchers: Arc<[String]>,
    card_index: Arc<HashMap<GameId, usize>>,
    launcher_index: Arc<HashMap<String, Arc<[usize]>>>,
    library_index: Arc<HashMap<String, Arc<[usize]>>>,
    addon_index: Arc<HashMap<String, Arc<[usize]>>>,
    hidden_count: usize,
}

impl CatalogSnapshot {
    pub(crate) fn build(
        context: &crate::Context,
        revision: CatalogRevision,
    ) -> Result<Self, ServiceError> {
        Self::build_with_facts_mode(context, revision, SnapshotFactsMode::Durable)
    }

    pub(crate) fn build_validated(
        context: &crate::Context,
        revision: CatalogRevision,
    ) -> Result<Self, ServiceError> {
        Self::build_with_facts_mode(context, revision, SnapshotFactsMode::ValidateLive)
    }

    fn build_with_facts_mode(
        context: &crate::Context,
        revision: CatalogRevision,
        facts_mode: SnapshotFactsMode,
    ) -> Result<Self, ServiceError> {
        builder::build_snapshot(context, revision, facts_mode)
    }

    /// Returns the process-local snapshot revision.
    #[must_use]
    pub const fn revision(&self) -> CatalogRevision {
        self.revision
    }

    /// Returns every immutable card projection.
    #[must_use]
    pub fn cards(&self) -> &[Arc<GameCardData>] {
        &self.cards
    }

    /// Returns library facets derived from the same card facts.
    #[must_use]
    pub fn available_libraries(&self) -> &[String] {
        &self.available_libraries
    }

    /// Returns launcher facets derived from the same game facts.
    #[must_use]
    pub fn available_launchers(&self) -> &[String] {
        &self.available_launchers
    }

    /// Looks up one card without scanning the snapshot.
    #[must_use]
    pub fn card(&self, game_id: &GameId) -> Option<&GameCardData> {
        self.card_index
            .get(game_id)
            .and_then(|index| self.cards.get(*index))
            .map(Arc::as_ref)
    }

    /// Returns candidate card indices using snapshot-owned facet indexes.
    /// Library and add-on selections form one OR group; launcher selection is
    /// intersected with that group. Returned indices are stable snapshot order.
    #[must_use]
    pub fn candidate_indices(
        &self,
        libraries: &[String],
        addons: &[String],
        launchers: &[String],
    ) -> Vec<usize> {
        let launcher_candidates = collect_index_union(&self.launcher_index, launchers);
        let mut content_filters = Vec::with_capacity(libraries.len() + addons.len());
        content_filters.extend(
            libraries
                .iter()
                .filter_map(|key| self.library_index.get(key))
                .flat_map(|indices| indices.iter().copied()),
        );
        content_filters.extend(
            addons
                .iter()
                .filter_map(|key| self.addon_index.get(key))
                .flat_map(|indices| indices.iter().copied()),
        );
        content_filters.sort_unstable();
        content_filters.dedup();

        match (
            launcher_candidates,
            libraries.is_empty() && addons.is_empty(),
        ) {
            (None, true) => (0..self.cards.len()).collect(),
            (Some(indices), true) => indices,
            (None, false) => content_filters,
            (Some(launchers), false) => intersect_sorted(&launchers, &content_filters),
        }
    }

    /// Total hidden-card facet derived from the same immutable snapshot.
    #[must_use]
    pub const fn hidden_count(&self) -> usize {
        self.hidden_count
    }

    pub(crate) fn with_cover_patch(
        &self,
        revision: CatalogRevision,
        game_id: &GameId,
        updated_at_ms: Option<i64>,
    ) -> Option<Self> {
        let index = *self.card_index.get(game_id)?;
        let mut cards = self.cards.to_vec();
        let mut card = cards.get(index)?.as_ref().clone();
        card.cover_updated_at_ms = updated_at_ms;
        cards[index] = Arc::new(card);

        Some(Self {
            revision,
            cards: cards.into(),
            ..self.clone()
        })
    }

    pub(crate) fn with_ui_state_patch(
        &self,
        revision: CatalogRevision,
        game_id: &GameId,
        is_favorite: bool,
        is_hidden: bool,
    ) -> Option<Self> {
        let index = *self.card_index.get(game_id)?;
        let mut cards = self.cards.to_vec();
        let mut card = cards.get(index)?.as_ref().clone();
        let was_hidden = card.is_hidden;
        card.is_favorite = is_favorite;
        card.is_hidden = is_hidden;
        cards[index] = Arc::new(card);

        Some(Self {
            revision,
            cards: cards.into(),
            hidden_count: self.hidden_count - usize::from(was_hidden) + usize::from(is_hidden),
            ..self.clone()
        })
    }

    pub(crate) fn changed_game_ids(&self, other: &Self) -> Vec<GameId> {
        let mut changed = BTreeSet::new();
        for card in self.cards.iter() {
            match other.card(card.game.id()) {
                Some(other_card) if card.as_ref() == other_card => {}
                Some(_) | None => {
                    changed.insert(card.game.id().clone());
                }
            }
        }
        for card in other.cards.iter() {
            if self.card(card.game.id()).is_none() {
                changed.insert(card.game.id().clone());
            }
        }
        changed.into_iter().collect()
    }

    pub(crate) fn preserving_cover_projection(mut self, current: &Self) -> Self {
        let mut cards = self.cards.to_vec();
        let mut changed = false;
        for card in &mut cards {
            let Some(current_card) = current.card(card.game.id()) else {
                continue;
            };
            if card.cover_updated_at_ms == current_card.cover_updated_at_ms {
                continue;
            }
            let mut updated = card.as_ref().clone();
            updated.cover_updated_at_ms = current_card.cover_updated_at_ms;
            *card = Arc::new(updated);
            changed = true;
        }
        if changed {
            self.cards = cards.into();
        }
        self
    }
}

/// Returns the current immutable snapshot. Rebuilds are single-flight; callers
/// arriving during a refresh keep using the previous snapshot.
pub(super) fn catalog_snapshot(
    context: &crate::Context,
) -> Result<Arc<CatalogSnapshot>, ServiceError> {
    context.catalog_snapshot()
}

/// Builds or waits for a snapshot matching the current storage generation.
///
/// This is reserved for refresh coordinators that must publish an authoritative
/// revision. Interactive readers should use [`catalog_snapshot`] so they can
/// continue rendering the previous immutable snapshot during a rebuild.
pub(super) fn refresh_catalog_snapshot(
    context: &crate::Context,
) -> Result<Arc<CatalogSnapshot>, ServiceError> {
    context.refresh_catalog_snapshot()
}

/// Rebuilds the card snapshot with fresh filesystem-sensitive facts.
///
/// Intended for background coordinators; interactive readers should never
/// wait for this validation.
pub(super) fn refresh_catalog_snapshot_validated(
    context: &crate::Context,
) -> Result<(Arc<CatalogSnapshot>, Vec<GameId>), ServiceError> {
    context.refresh_catalog_snapshot_validated()
}
