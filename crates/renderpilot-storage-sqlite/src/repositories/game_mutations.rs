//! One neutral SQLite commit boundary for coordinated game-file mutations.

use renderpilot_application::AppResult;
use renderpilot_domain::{
    AddonKind, ComponentId, ComponentRollbackBaseline, D3d12ExecutableBaseline,
    D3d12ExecutableIdentity, GameId, InstalledAddon, LibraryComponent,
};

use super::{
    SqliteStorage, component_backups, components, installed_addons, pending_file_mutations,
};

/// Typed mutation of the component rollback aggregate.
#[derive(Debug, Clone, Copy)]
pub enum ComponentBaselineMutation<'a> {
    /// Capture immutable original DLL/EXE identities once.
    Capture {
        /// Component receiving the baseline.
        component_id: &'a ComponentId,
        /// Exact pre-first-overlay rollback aggregate.
        baseline: &'a ComponentRollbackBaseline,
    },
    /// Capture the D3D12 executable original identity once for a DLL-only aggregate.
    CaptureD3d12Executable {
        /// Component receiving its first auxiliary baseline.
        component_id: &'a ComponentId,
        /// Immutable original and initial active executable identity.
        baseline: &'a D3d12ExecutableBaseline,
    },
    /// Change only the expected active D3D12 executable identity.
    UpdateD3d12ExecutableState {
        /// Component whose auxiliary state changed.
        component_id: &'a ComponentId,
        /// New active state; original path and identity remain storage-owned.
        expected_active: &'a D3d12ExecutableIdentity,
    },
    /// Update the last component-file identities committed by RenderPilot.
    ///
    /// This provenance lets cleanup validate an orphaned rollback aggregate
    /// after a later scan no longer detects the component row.
    UpdateExpectedActiveFiles {
        /// Component whose active identity changed.
        component_id: &'a ComponentId,
        /// Complete active file set produced by the committed replacement.
        files: &'a [renderpilot_domain::ComponentFile],
    },
    /// Delete the aggregate after a fully verified rollback.
    Delete {
        /// Component whose rollback aggregate was consumed.
        component_id: &'a ComponentId,
    },
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
    pub component_set: Option<&'a [LibraryComponent]>,
    /// Typed rollback aggregate mutations.
    pub baseline_mutations: &'a [ComponentBaselineMutation<'a>],
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
            for mutation in commit.baseline_mutations {
                match mutation {
                    ComponentBaselineMutation::Capture {
                        component_id,
                        baseline,
                    } => component_backups::capture_component_backup_within_transaction(
                        transaction,
                        commit.game_id,
                        component_id,
                        baseline,
                    )?,
                    ComponentBaselineMutation::UpdateD3d12ExecutableState {
                        component_id,
                        expected_active,
                    } => {
                        component_backups::update_component_d3d12_executable_state_within_transaction(
                            transaction,
                            component_id,
                            expected_active,
                        )?
                    }
                    ComponentBaselineMutation::UpdateExpectedActiveFiles {
                        component_id,
                        files,
                    } => component_backups::update_component_expected_active_files_within_transaction(
                        transaction,
                        component_id,
                        files,
                    )?,
                    ComponentBaselineMutation::CaptureD3d12Executable {
                        component_id,
                        baseline,
                    } => {
                        component_backups::capture_component_d3d12_executable_within_transaction(
                            transaction,
                            component_id,
                            baseline,
                        )?
                    }
                    ComponentBaselineMutation::Delete { component_id } => {
                        component_backups::delete_component_backup_within_transaction(
                            transaction,
                            component_id,
                        )?;
                    }
                }
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
                baseline_mutations: &[],
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
                baseline_mutations: &[],
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
