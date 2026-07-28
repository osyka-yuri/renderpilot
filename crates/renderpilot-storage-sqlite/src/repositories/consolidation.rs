//! Loss-aware migration of state from proven false legacy game cards.

use renderpilot_domain::GameId;

/// Explicit source-to-destination component identity mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentRekey {
    /// Component identity owned by the false source card.
    pub source_component_id: String,
    /// Matching component identity generated for the retained card.
    pub destination_component_id: String,
}

/// One false legacy card and its complete component identity mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsolidationSource {
    /// Proven false legacy card.
    pub source_game_id: GameId,
    /// Complete one-to-one mapping for every source component.
    pub component_rekeys: Vec<ComponentRekey>,
}

/// Fully validated consolidation input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsolidationPlan {
    /// Stable card that survives consolidation.
    pub destination_game_id: GameId,
    /// Proven false legacy cards to merge.
    pub sources: Vec<ConsolidationSource>,
}

impl ConsolidationPlan {
    /// Creates a no-op plan for a destination.
    #[must_use]
    pub fn empty(destination_game_id: GameId) -> Self {
        Self {
            destination_game_id,
            sources: Vec::new(),
        }
    }

    /// Whether the plan has no source cards.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

/// Read-only conflict preview used to gate the transaction on a durable recovery bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsolidationConflictSummary {
    /// Presentation-only state resolved by an explicit destination-wins policy.
    pub destination_wins_tables: Vec<String>,
    /// Managed state whose ambiguity must abort consolidation.
    pub blocking_tables: Vec<String>,
}

impl ConsolidationConflictSummary {
    /// Whether any conflict requires a durable recovery bundle.
    #[must_use]
    pub fn requires_recovery_bundle(&self) -> bool {
        !self.destination_wins_tables.is_empty() || !self.blocking_tables.is_empty()
    }

    /// Whether active managed state prevents consolidation.
    #[must_use]
    pub fn has_blocking_conflicts(&self) -> bool {
        !self.blocking_tables.is_empty()
    }

    /// Sorted tables included in the recovery manifest.
    #[must_use]
    pub fn recovery_tables(&self) -> Vec<String> {
        let mut tables = self.destination_wins_tables.clone();
        tables.extend(self.blocking_tables.iter().cloned());
        tables.sort();
        tables.dedup();
        tables
    }
}

/// Consolidation portion of a committed aggregate write.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsolidationReport {
    /// Source cards removed by the committed transaction.
    pub removed_game_ids: Vec<GameId>,
    /// Tables where the pre-existing destination row was retained.
    pub destination_wins_conflicts: Vec<String>,
    /// Cover files no longer referenced after commit.
    pub discarded_cover_file_names: Vec<String>,
}

/// Combined scan/consolidation write result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsolidatedScanWriteReport {
    /// Aggregate scan write counters.
    pub scan: super::ScanWriteReport,
    /// State-migration outcome.
    pub consolidation: ConsolidationReport,
}

mod execution;
mod inventory;
mod policy;
#[cfg(test)]
mod tests;
mod validation;

pub(super) use execution::{apply, ensure_conflicts_unchanged, verify_foreign_keys};
pub(super) use inventory::recovery_file_paths;
pub(super) use policy::inspect_conflicts;
