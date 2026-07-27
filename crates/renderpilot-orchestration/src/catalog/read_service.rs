//! Coherent catalog read boundary shared by presentation transports.

use std::sync::Arc;

use renderpilot_domain::GameId;

use crate::ServiceError;

use super::{CatalogSnapshot, GameDetailsCatalogResult};

/// Orchestration-owned entry point for catalog projections.
///
/// The service keeps transport layers out of SQLite details and guarantees that
/// cards, facets, revisions, and details use the same process-wide caches.
#[derive(Clone, Copy)]
pub struct CatalogReadService<'context> {
    context: &'context crate::Context,
}

impl<'context> CatalogReadService<'context> {
    /// Creates a read service over the process-wide application context.
    #[must_use]
    pub const fn new(context: &'context crate::Context) -> Self {
        Self { context }
    }

    /// Returns the latency-sensitive immutable catalog snapshot.
    pub fn snapshot(&self) -> Result<Arc<CatalogSnapshot>, ServiceError> {
        super::cards::catalog_snapshot(self.context)
    }

    /// Waits for a snapshot matching the current authoritative generation.
    pub fn refresh_snapshot(&self) -> Result<Arc<CatalogSnapshot>, ServiceError> {
        super::cards::refresh_catalog_snapshot(self.context)
    }

    /// Performs the mandatory background validation of filesystem-sensitive
    /// card facts and returns the ids whose effective projections changed.
    pub fn refresh_validated_snapshot(
        &self,
    ) -> Result<(Arc<CatalogSnapshot>, Vec<GameId>), ServiceError> {
        super::cards::refresh_catalog_snapshot_validated(self.context)
    }

    /// Returns the typed details projection backed by the shared universe and
    /// generation-keyed details cache.
    pub fn game_details(&self, game_id: &GameId) -> Result<GameDetailsCatalogResult, ServiceError> {
        super::get_game_details(self.context, game_id)
    }
}
