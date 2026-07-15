//! Persistence for crash-recoverable game-file mutation manifests.

use std::str::FromStr;

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::GameId;
use rusqlite::{OptionalExtension, Row, Transaction, named_params};

use crate::error::storage_error;
use crate::sqlite_clock;

use super::SqliteStorage;

/// Durable phase of a filesystem transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingFileMutationState {
    /// The row reserves the transaction id; game files have not been touched.
    Preparing,
    /// Before-snapshots exist and must be restored after a crash.
    Prepared,
    /// The feature database commit succeeded; only snapshot cleanup remains.
    Committed,
}

impl PendingFileMutationState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Prepared => "prepared",
            Self::Committed => "committed",
        }
    }
}

impl FromStr for PendingFileMutationState {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "preparing" => Ok(Self::Preparing),
            "prepared" => Ok(Self::Prepared),
            "committed" => Ok(Self::Committed),
            _ => Err(AppError::storage_failed(format!(
                "invalid pending file mutation state `{value}`"
            ))),
        }
    }
}

/// One pending mutation row. The JSON manifest is interpreted by orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFileMutationRow {
    /// Stable transaction id and snapshot-directory name.
    pub id: String,
    /// Game whose paths are protected by this transaction.
    pub game_id: GameId,
    /// Feature label such as `catalog_swap` or `luma_uninstall`.
    pub feature: String,
    /// Optional component or add-on identity.
    pub subject_id: Option<String>,
    /// Current durable transaction phase.
    pub state: PendingFileMutationState,
    /// Serialized before-snapshot manifest.
    pub manifest_json: String,
}

impl SqliteStorage {
    /// Inserts a prepared mutation before its first game-folder write.
    pub fn prepare_file_mutation(&self, row: &PendingFileMutationRow) -> AppResult<()> {
        self.with_connection(|connection| {
            let now_ms = sqlite_clock::now_ms(connection)?;
            connection
                .execute(
                    "
                    INSERT INTO pending_file_mutations
                        (id, game_id, feature, subject_id, state, manifest_json,
                         created_at, updated_at)
                    VALUES
                        (:id, :game_id, :feature, :subject_id, :state, :manifest_json,
                         :now_ms, :now_ms)
                    ",
                    named_params! {
                        ":id": row.id.as_str(),
                        ":game_id": row.game_id.as_str(),
                        ":feature": row.feature.as_str(),
                        ":subject_id": row.subject_id.as_deref(),
                        ":state": row.state.as_str(),
                        ":manifest_json": row.manifest_json.as_str(),
                        ":now_ms": now_ms,
                    },
                )
                .map_err(storage_error)?;
            Ok(())
        })
    }

    /// Lists unfinished or not-yet-cleaned mutations for one game oldest first.
    pub fn pending_file_mutations_for_game(
        &self,
        game_id: &GameId,
    ) -> AppResult<Vec<PendingFileMutationRow>> {
        self.query_list(
            "
            SELECT id, game_id, feature, subject_id, state, manifest_json
            FROM pending_file_mutations
            WHERE game_id = ?1
            ORDER BY created_at, id
            ",
            [game_id.as_str()],
            |row| Ok(row_to_pending_mutation(row)),
        )
    }

    /// Returns every reserved transaction id for orphan-directory sweeping.
    pub fn all_pending_file_mutation_ids(&self) -> AppResult<Vec<String>> {
        self.query_list(
            "SELECT id FROM pending_file_mutations ORDER BY created_at, id",
            [],
            |row| Ok(row.get::<_, String>(0).map_err(storage_error)),
        )
    }

    /// Publishes the completed before-snapshot manifest and opens the game-file
    /// mutation phase.
    pub fn finish_preparing_file_mutation(&self, id: &str, manifest_json: &str) -> AppResult<()> {
        self.with_transaction(|transaction| {
            let now_ms = sqlite_clock::now_ms(transaction)?;
            let updated = transaction
                .execute(
                    "
                    UPDATE pending_file_mutations
                    SET state = 'prepared', manifest_json = :manifest_json, updated_at = :now_ms
                    WHERE id = :id AND state = 'preparing'
                    ",
                    named_params! {
                        ":id": id,
                        ":manifest_json": manifest_json,
                        ":now_ms": now_ms,
                    },
                )
                .map_err(storage_error)?;
            if updated != 1 {
                return Err(AppError::storage_failed(format!(
                    "pending file mutation `{id}` is missing or is not preparing"
                )));
            }
            Ok(())
        })
    }

    /// Marks a prepared file-mutation row committed without an accompanying feature transaction.
    pub fn mark_file_mutation_committed(&self, id: &str) -> AppResult<()> {
        self.with_transaction(|transaction| {
            mark_file_mutation_committed_within_transaction(transaction, id)
        })
    }

    /// Deletes a recovered/cleaned mutation row.
    pub fn delete_pending_file_mutation(&self, id: &str) -> AppResult<()> {
        self.with_connection(|connection| {
            connection
                .execute("DELETE FROM pending_file_mutations WHERE id = ?1", [id])
                .map_err(storage_error)?;
            Ok(())
        })
    }

    /// Reads one mutation by id for tests and recovery diagnostics.
    pub fn get_pending_file_mutation(&self, id: &str) -> AppResult<Option<PendingFileMutationRow>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "
                    SELECT id, game_id, feature, subject_id, state, manifest_json
                    FROM pending_file_mutations WHERE id = ?1
                    ",
                    [id],
                    |row| Ok(row_to_pending_mutation(row)),
                )
                .optional()
                .map_err(storage_error)?
                .transpose()
        })
    }
}

pub(super) fn mark_file_mutation_committed_within_transaction(
    transaction: &Transaction<'_>,
    id: &str,
) -> AppResult<()> {
    let now_ms = sqlite_clock::now_ms(transaction)?;
    let updated = transaction
        .execute(
            "
            UPDATE pending_file_mutations
            SET state = 'committed', updated_at = :now_ms
            WHERE id = :id AND state = 'prepared'
            ",
            named_params! { ":id": id, ":now_ms": now_ms },
        )
        .map_err(storage_error)?;
    if updated != 1 {
        return Err(AppError::storage_failed(format!(
            "pending file mutation `{id}` is missing or is not prepared"
        )));
    }
    Ok(())
}

fn row_to_pending_mutation(row: &Row<'_>) -> AppResult<PendingFileMutationRow> {
    Ok(PendingFileMutationRow {
        id: row.get("id").map_err(storage_error)?,
        game_id: GameId::new(row.get::<_, String>("game_id").map_err(storage_error)?)
            .map_err(|error| AppError::storage_failed(error.to_string()))?,
        feature: row.get("feature").map_err(storage_error)?,
        subject_id: row.get("subject_id").map_err(storage_error)?,
        state: row
            .get::<_, String>("state")
            .map_err(storage_error)?
            .parse::<PendingFileMutationState>()?,
        manifest_json: row.get("manifest_json").map_err(storage_error)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_row_round_trips_and_commits() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let row = PendingFileMutationRow {
            id: "tx-1".to_owned(),
            game_id: GameId::new("steam:1").expect("id"),
            feature: renderpilot_domain::mutation_features::CATALOG_SWAP.to_owned(),
            subject_id: Some("component:1".to_owned()),
            state: PendingFileMutationState::Prepared,
            manifest_json: r#"{"snapshots":[]}"#.to_owned(),
        };

        storage.prepare_file_mutation(&row).expect("prepare");
        assert_eq!(
            storage.get_pending_file_mutation("tx-1").expect("get"),
            Some(row)
        );

        storage
            .mark_file_mutation_committed("tx-1")
            .expect("commit");
        assert_eq!(
            storage
                .get_pending_file_mutation("tx-1")
                .expect("get")
                .expect("row")
                .state,
            PendingFileMutationState::Committed
        );
    }

    #[test]
    fn preparing_row_publishes_its_manifest_before_commit() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let mut row = PendingFileMutationRow {
            id: "tx-preparing".to_owned(),
            game_id: GameId::new("steam:2").expect("id"),
            feature: renderpilot_domain::mutation_features::CATALOG_SWAP.to_owned(),
            subject_id: None,
            state: PendingFileMutationState::Preparing,
            manifest_json:
                r#"{"format_version":1,"roots":[],"transaction_dir":"unused","snapshots":[]}"#
                    .to_owned(),
        };

        storage.prepare_file_mutation(&row).expect("reserve");
        row.manifest_json =
            r#"{"format_version":1,"roots":["C:/game"],"transaction_dir":"C:/tx","snapshots":[]}"#
                .to_owned();
        storage
            .finish_preparing_file_mutation(&row.id, &row.manifest_json)
            .expect("publish");

        let stored = storage
            .get_pending_file_mutation(&row.id)
            .expect("read")
            .expect("row");
        assert_eq!(stored.state, PendingFileMutationState::Prepared);
        assert_eq!(stored.manifest_json, row.manifest_json);
    }

    #[test]
    fn illegal_state_transitions_are_rejected() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let row = PendingFileMutationRow {
            id: "tx-illegal".to_owned(),
            game_id: GameId::new("steam:3").expect("id"),
            feature: renderpilot_domain::mutation_features::CATALOG_SWAP.to_owned(),
            subject_id: None,
            state: PendingFileMutationState::Preparing,
            manifest_json: r#"{"snapshots":[]}"#.to_owned(),
        };
        storage.prepare_file_mutation(&row).expect("reserve");

        storage
            .mark_file_mutation_committed("tx-illegal")
            .expect_err("cannot commit from preparing");
        assert_eq!(
            storage
                .get_pending_file_mutation("tx-illegal")
                .expect("get")
                .expect("row")
                .state,
            PendingFileMutationState::Preparing
        );

        storage
            .finish_preparing_file_mutation("tx-illegal", r#"{"snapshots":[]}"#)
            .expect("preparing -> prepared");
        storage
            .finish_preparing_file_mutation("tx-illegal", r#"{"snapshots":[]}"#)
            .expect_err("cannot finish preparing twice");
        storage
            .mark_file_mutation_committed("tx-illegal")
            .expect("prepared -> committed");
        storage
            .mark_file_mutation_committed("tx-illegal")
            .expect_err("cannot commit twice");
        assert_eq!(
            storage
                .get_pending_file_mutation("tx-illegal")
                .expect("get")
                .expect("row")
                .state,
            PendingFileMutationState::Committed
        );
    }

    #[test]
    fn rust_state_strings_match_sql_check_constraint() {
        // Keep in sync with CHECK (state IN (...)) in schema/ddl/pending_file_mutations.
        let allowed = ["preparing", "prepared", "committed"];
        for state in [
            PendingFileMutationState::Preparing,
            PendingFileMutationState::Prepared,
            PendingFileMutationState::Committed,
        ] {
            assert!(
                allowed.contains(&state.as_str()),
                "state {:?} missing from SQL CHECK set",
                state
            );
            assert_eq!(
                state
                    .as_str()
                    .parse::<PendingFileMutationState>()
                    .expect("round-trip"),
                state
            );
        }
        assert!("done".parse::<PendingFileMutationState>().is_err());
    }
}
