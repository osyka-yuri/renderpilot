//! One neutral SQLite commit boundary for coordinated game-file mutations.

use renderpilot_application::AppResult;
use renderpilot_domain::{
    AddonKind, ComponentId, ComponentRollbackBaseline, D3d12ExecutableBaseline,
    D3d12ExecutableIdentity, GameId, InstalledAddon, LibraryComponent,
};

use super::{
    SqliteStorage, component_backups, components, installed_addons, observations,
    pending_file_mutations,
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
            let prepared_binding = if let Some(mutation_id) = commit.mutation_id {
                Some(pending_file_mutations::validate_prepared_mutation_commit_within_transaction(
                    transaction,
                    commit.game_id,
                    mutation_id,
                    commit.component_set.map(<[LibraryComponent]>::len),
                    !commit.baseline_mutations.is_empty(),
                )?)
            } else {
                None
            };
            let skip_absent_empty_component_replacement = matches!(
                (prepared_binding, commit.component_set),
                (
                    Some(pending_file_mutations::PreparedMutationCommitBinding::CatalogAbsent),
                    Some(component_set)
                ) if component_set.is_empty()
            );
            if let Some(component_set) = commit.component_set
                && !skip_absent_empty_component_replacement
            {
                components::replace_components_for_game_within_transaction(
                    transaction,
                    commit.game_id,
                    component_set,
                )?;
                if commit.mutation_id.is_none() {
                    observations::invalidate_game_authority_within_transaction(
                        transaction,
                        commit.game_id,
                        "game_mutation_component_set",
                        None,
                    )?;
                }
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
    use renderpilot_application::{ComponentRepository, GameRepository, InstalledAddonRepository};
    use renderpilot_domain::{
        AddonKind, ComponentKind, GameId, GameIdentity, GameInstallation, GameRuntime,
        InstalledAddon, Launcher, LibraryComponent, LibraryTechnology, PathRef, Platform,
        Swappability,
    };

    use super::*;
    use crate::repositories::{
        AuthorityCas, BeginFileMutationPreparation, CatalogReadiness, CompleteScanWriteUnit,
        PendingFileMutationRow, PendingFileMutationState, SqliteStorage,
    };

    fn test_game(game_id: GameId) -> GameInstallation {
        let install_path =
            PathRef::new(format!("C:/Games/{}", game_id.as_str().replace(':', "_"))).expect("path");
        let identity = GameIdentity::new(game_id, "Test Game", Launcher::Steam).expect("identity");
        GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            install_path,
        )
    }

    fn complete_game_scan(storage: &SqliteStorage, game: &GameInstallation) {
        storage
            .save_complete_scan_write_unit(CompleteScanWriteUnit {
                game,
                components: &[],
                artifacts: &[],
                observations: &[],
                authority: AuthorityCas::new(0),
                prune_empty_operations: false,
            })
            .expect("complete scan");
    }

    fn prepare_mutation(storage: &SqliteStorage, game_id: &GameId, id: &str) {
        storage
            .prepare_file_mutation(&PendingFileMutationRow {
                id: id.to_owned(),
                game_id: game_id.clone(),
                feature: renderpilot_domain::mutation_features::LUMA_UPDATE.to_owned(),
                subject_id: None,
                state: PendingFileMutationState::Preparing,
                manifest_json: r#"{"snapshots":[]}"#.to_owned(),
            })
            .expect("reserve mutation");
        storage
            .finish_preparing_file_mutation(id, r#"{"snapshots":[]}"#)
            .expect("prepare mutation");
    }

    fn prepare_pre_catalog_mutation(storage: &SqliteStorage, game_id: &GameId, id: &str) {
        storage
            .begin_file_mutation_preparation(&BeginFileMutationPreparation {
                id: id.to_owned(),
                game_id: game_id.clone(),
                feature: renderpilot_domain::mutation_features::LUMA_UPDATE.to_owned(),
                subject_id: None,
                initial_manifest_json: r#"{"snapshots":[]}"#.to_owned(),
            })
            .expect("reserve pre-catalog mutation");
        storage
            .finish_preparing_file_mutation(id, r#"{"snapshots":[]}"#)
            .expect("prepare pre-catalog mutation");
    }

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
        storage
            .upsert_game(&test_game(game_id.clone()))
            .expect("store game");
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
                state: PendingFileMutationState::Preparing,
                manifest_json: r#"{"snapshots":[]}"#.to_owned(),
            })
            .expect("prepare");
        storage
            .finish_preparing_file_mutation("tx-ok", r#"{"snapshots":[]}"#)
            .expect("finish prepare");

        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: &game_id,
                component_set: Some(&[]),
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
        assert_eq!(
            storage.catalog_readiness(&game_id).expect("readiness"),
            CatalogReadiness::Invalidated {
                authority_epoch: 1,
                reason: "prepared_file_mutation".to_owned(),
                mutation_token: Some("tx-ok".to_owned()),
            }
        );
    }

    #[test]
    fn metadata_only_mutation_leaves_a_complete_authority_unchanged() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new("steam:metadata-only").expect("id");
        let game = test_game(game_id.clone());
        complete_game_scan(&storage, &game);

        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: &game_id,
                component_set: None,
                baseline_mutations: &[],
                addon: InstalledAddonMutation::Keep,
                mutation_id: None,
            })
            .expect("metadata-only mutation");

        let CatalogReadiness::Complete(ready) =
            storage.catalog_readiness(&game_id).expect("readiness")
        else {
            panic!("metadata-only mutation must preserve Complete authority");
        };
        assert_eq!(ready.game_id(), &game_id);
        assert_eq!(ready.authority_epoch(), 1);
    }

    #[test]
    fn component_set_without_file_mutation_invalidates_authority() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new("steam:component-set").expect("id");
        let game = test_game(game_id.clone());
        complete_game_scan(&storage, &game);

        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: &game_id,
                component_set: Some(&[]),
                baseline_mutations: &[],
                addon: InstalledAddonMutation::Keep,
                mutation_id: None,
            })
            .expect("component mutation");

        assert_eq!(
            storage.catalog_readiness(&game_id).expect("readiness"),
            CatalogReadiness::Invalidated {
                authority_epoch: 2,
                reason: "game_mutation_component_set".to_owned(),
                mutation_token: None,
            }
        );
    }

    #[test]
    fn mutation_id_requires_same_game_prepared_row_with_matching_invalidation() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new("steam:mutation-conditions").expect("id");
        let other_game_id = GameId::new("steam:mutation-other-game").expect("id");
        let game = test_game(game_id.clone());
        let other_game = test_game(other_game_id.clone());
        complete_game_scan(&storage, &game);
        complete_game_scan(&storage, &other_game);
        prepare_mutation(&storage, &other_game_id, "tx-other-game");

        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: &game_id,
                component_set: Some(&[]),
                baseline_mutations: &[],
                addon: InstalledAddonMutation::Keep,
                mutation_id: Some("tx-other-game"),
            })
            .expect_err("a different game's prepared mutation cannot commit");

        storage
            .prepare_file_mutation(&PendingFileMutationRow {
                id: "tx-without-invalidation".to_owned(),
                game_id: game_id.clone(),
                feature: renderpilot_domain::mutation_features::LUMA_UPDATE.to_owned(),
                subject_id: None,
                state: PendingFileMutationState::Prepared,
                manifest_json: r#"{"snapshots":[]}"#.to_owned(),
            })
            .expect("fixture prepared row");
        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: &game_id,
                component_set: None,
                baseline_mutations: &[],
                addon: InstalledAddonMutation::Keep,
                mutation_id: Some("tx-without-invalidation"),
            })
            .expect_err("prepared state without matching invalidation cannot commit");
        let CatalogReadiness::Complete(ready) =
            storage.catalog_readiness(&game_id).expect("readiness")
        else {
            panic!("rejected mutation must preserve Complete authority");
        };
        assert_eq!(ready.game_id(), &game_id);
        assert_eq!(ready.authority_epoch(), 1);
    }

    #[test]
    fn pre_catalog_mutation_permits_only_addon_commit_effects() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new("steam:pre-catalog-addon").expect("id");
        prepare_pre_catalog_mutation(&storage, &game_id, "tx-pre-catalog-addon");
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
                mutation_id: Some("tx-pre-catalog-addon"),
            })
            .expect("pre-catalog add-on commit");

        assert!(
            storage
                .get_installed_addon(&game_id)
                .expect("addon")
                .is_some()
        );
        assert_eq!(
            storage
                .get_pending_file_mutation("tx-pre-catalog-addon")
                .expect("row")
                .expect("row")
                .state,
            PendingFileMutationState::Committed
        );
        assert!(storage.catalog_readiness(&game_id).is_err());
    }

    #[test]
    fn pre_catalog_mutation_allows_empty_component_cleanup_without_creating_catalog_state() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new("steam:pre-catalog-empty-cleanup").expect("id");
        let addon = InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            PathRef::new(r"C:\Games\Test\Luma-Game.addon").expect("path"),
        );
        storage
            .upsert_installed_addon(&addon)
            .expect("seed orphan add-on");
        prepare_pre_catalog_mutation(&storage, &game_id, "tx-pre-catalog-empty-cleanup");

        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: &game_id,
                component_set: Some(&[]),
                baseline_mutations: &[],
                addon: InstalledAddonMutation::Delete(AddonKind::Luma),
                mutation_id: Some("tx-pre-catalog-empty-cleanup"),
            })
            .expect("empty orphan component cleanup is a no-op");

        assert!(storage.find_game(&game_id).expect("game query").is_none());
        assert!(storage.catalog_readiness(&game_id).is_err());
        assert!(
            storage
                .list_components_for_game(&game_id)
                .expect("component query")
                .is_empty()
        );
        assert!(
            storage
                .get_installed_addon(&game_id)
                .expect("add-on query")
                .is_none()
        );
    }

    #[test]
    fn pre_catalog_mutation_rejects_nonempty_component_and_baseline_effects() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new("steam:pre-catalog-catalog-write").expect("id");
        prepare_pre_catalog_mutation(&storage, &game_id, "tx-pre-catalog-nonempty");

        let component = LibraryComponent::new(
            ComponentId::new("component:pre-catalog-set").expect("component id"),
            game_id.clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::DlssSuperResolution,
            Swappability::BundleOnly,
        );
        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: &game_id,
                component_set: Some(std::slice::from_ref(&component)),
                baseline_mutations: &[],
                addon: InstalledAddonMutation::Keep,
                mutation_id: Some("tx-pre-catalog-nonempty"),
            })
            .expect_err("pre-catalog component write must fail");

        let component_id = ComponentId::new("component:pre-catalog").expect("component id");
        let baseline = [ComponentBaselineMutation::Delete {
            component_id: &component_id,
        }];
        prepare_pre_catalog_mutation(&storage, &game_id, "tx-pre-catalog-baseline");
        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: &game_id,
                component_set: None,
                baseline_mutations: &baseline,
                addon: InstalledAddonMutation::Keep,
                mutation_id: Some("tx-pre-catalog-baseline"),
            })
            .expect_err("pre-catalog baseline write must fail");
        assert_eq!(
            storage
                .get_pending_file_mutation("tx-pre-catalog-nonempty")
                .expect("row")
                .expect("row")
                .state,
            PendingFileMutationState::Prepared
        );
        assert_eq!(
            storage
                .get_pending_file_mutation("tx-pre-catalog-baseline")
                .expect("row")
                .expect("row")
                .state,
            PendingFileMutationState::Prepared
        );
    }

    #[test]
    fn late_catalog_insertion_binds_prepared_addon_commit_before_feature_writes() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new("steam:late-catalog-binding").expect("id");
        prepare_pre_catalog_mutation(&storage, &game_id, "tx-late-catalog-binding");
        let game = test_game(game_id.clone());
        storage.upsert_game(&game).expect("late game insert");
        let addon = InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            PathRef::new(r"C:\Games\Test\Luma-Game.addon").expect("path"),
        );

        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: &game_id,
                component_set: Some(&[]),
                baseline_mutations: &[],
                addon: InstalledAddonMutation::Upsert(&addon),
                mutation_id: Some("tx-late-catalog-binding"),
            })
            .expect("late-bound add-on commit");
        assert_eq!(
            storage.catalog_readiness(&game_id).expect("authority"),
            CatalogReadiness::Invalidated {
                authority_epoch: 1,
                reason: "prepared_file_mutation".to_owned(),
                mutation_token: Some("tx-late-catalog-binding".to_owned()),
            }
        );
    }

    #[test]
    fn complete_scan_publication_is_excluded_while_late_bound_mutation_is_pending() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new("steam:pending-complete-exclusion").expect("id");
        prepare_pre_catalog_mutation(&storage, &game_id, "tx-pending-complete-exclusion");
        let game = test_game(game_id.clone());
        storage.upsert_game(&game).expect("late game insert");

        storage
            .save_complete_scan_write_unit(CompleteScanWriteUnit {
                game: &game,
                components: &[],
                artifacts: &[],
                observations: &[],
                authority: AuthorityCas::new(0),
                prune_empty_operations: false,
            })
            .expect_err("a pending file mutation must exclude Complete publication");
        assert!(matches!(
            storage.catalog_readiness(&game_id).expect("authority"),
            CatalogReadiness::NeverCompleted { authority_epoch: 0 }
        ));
    }
}
