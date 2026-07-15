//! One neutral SQLite commit boundary for coordinated game-file mutations.

use renderpilot_application::AppResult;
use renderpilot_domain::{
    AddonKind, ComponentFile, ComponentId, GameId, GraphicsComponent, InstalledAddon,
};

use super::{
    SqliteStorage, component_backups, components, installed_addons, pending_file_mutations,
};

/// Immutable baseline row inserted with a coordinated feature commit.
#[derive(Debug, Clone, Copy)]
pub struct ComponentBaselineInsert<'a> {
    /// Component receiving the baseline.
    pub component_id: &'a ComponentId,
    /// Exact pre-first-overlay file identities.
    pub files: &'a [ComponentFile],
}

/// Optional installed-add-on row change in the same transaction.
#[derive(Debug, Clone, Copy, Default)]
pub enum InstalledAddonMutation<'a> {
    /// Leave the installed-add-on table unchanged.
    #[default]
    Keep,
    /// Insert or replace one validated record.
    Upsert(&'a InstalledAddon),
    /// Delete the selected kind for the commit's game.
    Delete(AddonKind),
}

/// Complete database half of one durable game-file transaction.
#[derive(Debug, Clone, Copy)]
pub struct GameMutationCommit<'a> {
    /// Game whose coordinated state is changing.
    pub game_id: &'a GameId,
    /// Optional full component-set replacement.
    pub component_set: Option<&'a [GraphicsComponent]>,
    /// Baselines created by a first catalog overlay.
    pub baseline_inserts: &'a [ComponentBaselineInsert<'a>],
    /// Baselines consumed by catalog rollback.
    pub baseline_deletes: &'a [ComponentId],
    /// Optional installed-add-on mutation.
    pub addon: InstalledAddonMutation<'a>,
    /// Durable filesystem mutation row to mark committed with the feature rows.
    ///
    /// `None` is for metadata-only commits where no game-file root is reachable
    /// (orphaned install cleanup): feature rows still commit atomically, but no
    /// pending file-mutation row is required.
    pub mutation_id: Option<&'a str>,
}

impl SqliteStorage {
    /// Atomically commits feature rows and, when present, the durable filesystem phase.
    pub fn commit_game_mutation(&self, commit: GameMutationCommit<'_>) -> AppResult<()> {
        self.with_transaction(|transaction| {
            if let Some(component_set) = commit.component_set {
                components::replace_components_for_game_within_transaction(
                    transaction,
                    commit.game_id,
                    component_set,
                )?;
            }
            for insert in commit.baseline_inserts {
                component_backups::set_component_backup_within_transaction(
                    transaction,
                    commit.game_id,
                    insert.component_id,
                    insert.files,
                )?;
            }
            for component_id in commit.baseline_deletes {
                component_backups::delete_component_backup_within_transaction(
                    transaction,
                    component_id,
                )?;
            }
            match commit.addon {
                InstalledAddonMutation::Keep => {}
                InstalledAddonMutation::Upsert(addon) => {
                    installed_addons::upsert_within_transaction(transaction, addon)?;
                }
                InstalledAddonMutation::Delete(kind) => {
                    installed_addons::delete_within_transaction(transaction, commit.game_id, kind)?;
                }
            }
            if let Some(mutation_id) = commit.mutation_id {
                pending_file_mutations::mark_file_mutation_committed_within_transaction(
                    transaction,
                    mutation_id,
                )?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_application::InstalledAddonRepository;
    use renderpilot_domain::{AddonKind, GameId, InstalledAddon, PathRef};

    use super::*;
    use crate::repositories::{PendingFileMutationRow, PendingFileMutationState, SqliteStorage};

    #[test]
    fn commit_game_mutation_rolls_back_addon_when_mutation_mark_fails() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new("steam:mutation-atomic").expect("id");
        let addon = InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            PathRef::new(r"C:\Games\Test\Luma-Game.addon").expect("path"),
        );

        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: &game_id,
                component_set: None,
                baseline_inserts: &[],
                baseline_deletes: &[],
                addon: InstalledAddonMutation::Upsert(&addon),
                mutation_id: Some("missing-tx"),
            })
            .expect_err("missing mutation id must fail the whole commit");

        assert!(
            storage
                .get_installed_addon(&game_id)
                .expect("query")
                .is_none(),
            "addon upsert must roll back when mutation mark fails"
        );
    }

    #[test]
    fn commit_game_mutation_marks_prepared_mutation_with_addon_upsert() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new("steam:mutation-ok").expect("id");
        let addon = InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            PathRef::new(r"C:\Games\Test\Luma-Game.addon").expect("path"),
        );
        storage
            .prepare_file_mutation(&PendingFileMutationRow {
                id: "tx-ok".to_owned(),
                game_id: game_id.clone(),
                feature: renderpilot_domain::mutation_features::LUMA_UPDATE.to_owned(),
                subject_id: None,
                state: PendingFileMutationState::Prepared,
                manifest_json: r#"{"snapshots":[]}"#.to_owned(),
            })
            .expect("prepare");

        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: &game_id,
                component_set: None,
                baseline_inserts: &[],
                baseline_deletes: &[],
                addon: InstalledAddonMutation::Upsert(&addon),
                mutation_id: Some("tx-ok"),
            })
            .expect("commit");

        assert!(
            storage
                .get_installed_addon(&game_id)
                .expect("query")
                .is_some()
        );
        assert_eq!(
            storage
                .get_pending_file_mutation("tx-ok")
                .expect("get")
                .expect("row")
                .state,
            PendingFileMutationState::Committed
        );
    }
}
