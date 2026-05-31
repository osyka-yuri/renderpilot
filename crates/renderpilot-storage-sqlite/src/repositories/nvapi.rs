//! NVAPI overrides and baselines tables.
//!
//! See `migrations/0002_nvapi.sql` for the schema. Both tables CASCADE
//! on `games.id`, so deleting a game also tears down its NVAPI state.

use renderpilot_application::AppResult;
use rusqlite::{params, OptionalExtension};

use crate::{error::storage_context, SqliteStorage};

// -----------------------------------------------------------------------------
// Executable override
// -----------------------------------------------------------------------------

/// One row from `nvapi_executable_overrides`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvapiExecutableOverrideRow {
    /// Game this override applies to.
    pub game_id: String,
    /// Absolute path to the chosen executable on disk.
    pub selected_path: String,
    /// Basename (filename only) — what gets passed to NVAPI.
    pub selected_basename: String,
    /// Unix milliseconds of the last write.
    pub updated_at: i64,
}

// -----------------------------------------------------------------------------
// Setting baseline
// -----------------------------------------------------------------------------

/// Represents a single immutable row within the `nvapi_setting_baselines` table.
/// This acts as a historical snapshot recorded by RenderPilot immediately prior to
/// modifying a specific `(game, setting)` pair for the first time. To ensure
/// fidelity of the "revert to baseline" functionality, this snapshot is strictly
/// preserved and never overwritten by subsequent modification attempts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvapiSettingBaselineRow {
    /// Game this baseline applies to.
    pub game_id: String,
    /// Wire-stable setting identifier, e.g. `"dlss_sr_render_preset"`.
    pub setting_key: String,
    /// The DWORD value the setting had right before the first write.
    pub baseline_dword: u32,
    /// Whether the captured value matched the driver's predefined
    /// value (`true`) — i.e. nothing had touched it before — or
    /// differed (`false`, meaning another tool had already applied
    /// an override before RenderPilot saw it).
    pub baseline_was_predefined: bool,
    /// The driver-predefined value at capture time, if NVAPI reported
    /// `isPredefinedValid`. `None` when the driver had no opinion.
    pub predefined_dword: Option<u32>,
    /// Executable basename used when capturing.
    pub captured_exe: String,
    /// Unix milliseconds of capture.
    pub captured_at: i64,
}

// -----------------------------------------------------------------------------
// SqliteStorage methods
// -----------------------------------------------------------------------------

impl SqliteStorage {
    /// Inserts or replaces the executable override for `game_id`.
    pub fn upsert_nvapi_executable_override(
        &self,
        game_id: &str,
        selected_path: &str,
        selected_basename: &str,
    ) -> AppResult<()> {
        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO nvapi_executable_overrides
                    (game_id, selected_path, selected_basename, updated_at)
                 VALUES (?1, ?2, ?3, CAST(unixepoch('subsec') * 1000 AS INTEGER))
                 ON CONFLICT(game_id) DO UPDATE SET
                    selected_path     = excluded.selected_path,
                    selected_basename = excluded.selected_basename,
                    updated_at        = excluded.updated_at",
                params![game_id, selected_path, selected_basename],
            )
            .map(|_| ())
            .map_err(|error| storage_context("could not upsert nvapi executable override", error))
    }

    /// Returns the executable override for `game_id`, if any.
    pub fn get_nvapi_executable_override(
        &self,
        game_id: &str,
    ) -> AppResult<Option<NvapiExecutableOverrideRow>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT game_id, selected_path, selected_basename, updated_at
                 FROM nvapi_executable_overrides
                 WHERE game_id = ?1",
                params![game_id],
                |row| {
                    Ok(NvapiExecutableOverrideRow {
                        game_id: row.get(0)?,
                        selected_path: row.get(1)?,
                        selected_basename: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(|error| storage_context("could not read nvapi executable override", error))
    }

    /// Deletes the executable override for `game_id`, if any.
    pub fn delete_nvapi_executable_override(&self, game_id: &str) -> AppResult<()> {
        let connection = self.connection()?;
        connection
            .execute(
                "DELETE FROM nvapi_executable_overrides WHERE game_id = ?1",
                params![game_id],
            )
            .map(|_| ())
            .map_err(|error| storage_context("could not delete nvapi executable override", error))
    }

    /// Records an initial baseline snapshot exclusively if no preexisting record
    /// is found for the specified `(game_id, setting_key)` tuple. Yields `true`
    /// upon successfully inserting a new snapshot, or `false` if a baseline was
    /// already recorded.
    ///
    /// This mechanism serves as the foundational pillar for the "revert to baseline"
    /// capability. Because the captured value strictly reflects the driver's state
    /// *before* any RenderPilot intervention, subsequent application-driven writes
    /// will purposefully bypass this function, safeguarding the original baseline.
    pub fn capture_nvapi_baseline_if_missing(
        &self,
        game_id: &str,
        setting_key: &str,
        baseline_dword: u32,
        baseline_was_predefined: bool,
        predefined_dword: Option<u32>,
        captured_exe: &str,
    ) -> AppResult<bool> {
        let connection = self.connection()?;
        let rows_affected = connection
            .execute(
                "INSERT INTO nvapi_setting_baselines
                    (game_id, setting_key, baseline_dword, baseline_was_predefined,
                     predefined_dword, captured_exe, captured_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                     CAST(unixepoch('subsec') * 1000 AS INTEGER))
                 ON CONFLICT (game_id, setting_key) DO NOTHING",
                params![
                    game_id,
                    setting_key,
                    baseline_dword,
                    if baseline_was_predefined { 1 } else { 0 },
                    predefined_dword,
                    captured_exe,
                ],
            )
            .map_err(|error| storage_context("could not capture nvapi setting baseline", error))?;
        Ok(rows_affected > 0)
    }

    /// Returns the baseline row for `(game_id, setting_key)`, if any.
    pub fn get_nvapi_baseline(
        &self,
        game_id: &str,
        setting_key: &str,
    ) -> AppResult<Option<NvapiSettingBaselineRow>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT game_id, setting_key, baseline_dword, baseline_was_predefined,
                        predefined_dword, captured_exe, captured_at
                 FROM nvapi_setting_baselines
                 WHERE game_id = ?1 AND setting_key = ?2",
                params![game_id, setting_key],
                |row| {
                    let was_predefined: i32 = row.get(3)?;
                    Ok(NvapiSettingBaselineRow {
                        game_id: row.get(0)?,
                        setting_key: row.get(1)?,
                        baseline_dword: row.get(2)?,
                        baseline_was_predefined: was_predefined != 0,
                        predefined_dword: row.get(4)?,
                        captured_exe: row.get(5)?,
                        captured_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(|error| storage_context("could not read nvapi setting baseline", error))
    }

    /// Erases any existing baseline snapshot associated with the `(game_id, setting_key)`
    /// tuple. This capability is primarily utilized within testing environments, as
    /// production workflows generally require the baseline to be durably persisted
    /// for the entire lifecycle of the game's registration.
    pub fn delete_nvapi_baseline(&self, game_id: &str, setting_key: &str) -> AppResult<()> {
        let connection = self.connection()?;
        connection
            .execute(
                "DELETE FROM nvapi_setting_baselines
                 WHERE game_id = ?1 AND setting_key = ?2",
                params![game_id, setting_key],
            )
            .map(|_| ())
            .map_err(|error| storage_context("could not delete nvapi setting baseline", error))
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };

    use crate::SqliteStorage;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_storage() -> (SqliteStorage, PathBuf, GameId) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("renderpilot-nvapi-test-{nanos}-{counter}.db"));
        // Ensure clean slate.
        let _ = std::fs::remove_file(&path);
        let storage = SqliteStorage::open(&path).expect("open SqliteStorage");
        let game_id = GameId::new("manual:test-game").expect("game id");
        let identity =
            GameIdentity::new(game_id.clone(), "Test Game", Launcher::Manual).expect("identity");
        let installation = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new("C:/Games/Test").expect("install path"),
        );
        storage
            .upsert_game(&installation)
            .expect("upsert seed game");
        (storage, path, game_id)
    }

    #[test]
    fn executable_override_roundtrip() {
        let (storage, _path, game_id) = fresh_storage();

        assert!(storage
            .get_nvapi_executable_override(game_id.as_str())
            .unwrap()
            .is_none());

        storage
            .upsert_nvapi_executable_override(
                game_id.as_str(),
                "C:/Games/Test/Game.exe",
                "Game.exe",
            )
            .unwrap();

        let row = storage
            .get_nvapi_executable_override(game_id.as_str())
            .unwrap()
            .expect("row present");
        assert_eq!(row.selected_basename, "Game.exe");
        assert_eq!(row.selected_path, "C:/Games/Test/Game.exe");
        assert!(row.updated_at > 0);
    }

    #[test]
    fn executable_override_upsert_replaces_existing() {
        let (storage, _path, game_id) = fresh_storage();

        storage
            .upsert_nvapi_executable_override(game_id.as_str(), "C:/Games/Test/Old.exe", "Old.exe")
            .unwrap();
        storage
            .upsert_nvapi_executable_override(game_id.as_str(), "C:/Games/Test/New.exe", "New.exe")
            .unwrap();

        let row = storage
            .get_nvapi_executable_override(game_id.as_str())
            .unwrap()
            .unwrap();
        assert_eq!(row.selected_basename, "New.exe");
    }

    #[test]
    fn delete_executable_override_removes_row() {
        let (storage, _path, game_id) = fresh_storage();

        storage
            .upsert_nvapi_executable_override(
                game_id.as_str(),
                "C:/Games/Test/Game.exe",
                "Game.exe",
            )
            .unwrap();
        storage
            .delete_nvapi_executable_override(game_id.as_str())
            .unwrap();

        assert!(storage
            .get_nvapi_executable_override(game_id.as_str())
            .unwrap()
            .is_none());
    }

    #[test]
    fn capture_baseline_inserts_only_once() {
        let (storage, _path, game_id) = fresh_storage();
        let key = "dlss_sr_render_preset";

        let first = storage
            .capture_nvapi_baseline_if_missing(game_id.as_str(), key, 0, true, Some(0), "Game.exe")
            .unwrap();
        assert!(first, "first call should insert");

        // Try to capture a different value; existing baseline must be preserved.
        let second = storage
            .capture_nvapi_baseline_if_missing(game_id.as_str(), key, 6, false, Some(0), "Game.exe")
            .unwrap();
        assert!(!second, "second call should be a no-op");

        let row = storage
            .get_nvapi_baseline(game_id.as_str(), key)
            .unwrap()
            .expect("baseline row");
        assert_eq!(row.baseline_dword, 0);
        assert!(row.baseline_was_predefined);
        assert_eq!(row.predefined_dword, Some(0));
    }

    #[test]
    fn get_baseline_returns_none_when_absent() {
        let (storage, _path, game_id) = fresh_storage();
        assert!(storage
            .get_nvapi_baseline(game_id.as_str(), "missing.setting")
            .unwrap()
            .is_none());
    }

    #[test]
    fn delete_baseline_removes_row() {
        let (storage, _path, game_id) = fresh_storage();
        let key = "dlss_sr_render_preset";

        storage
            .capture_nvapi_baseline_if_missing(game_id.as_str(), key, 5, false, None, "Game.exe")
            .unwrap();
        storage
            .delete_nvapi_baseline(game_id.as_str(), key)
            .unwrap();
        assert!(storage
            .get_nvapi_baseline(game_id.as_str(), key)
            .unwrap()
            .is_none());
    }

    #[test]
    fn baseline_persists_predefined_dword_as_null_when_none() {
        let (storage, _path, game_id) = fresh_storage();
        let key = "dlss_sr_render_preset";

        storage
            .capture_nvapi_baseline_if_missing(game_id.as_str(), key, 3, false, None, "Game.exe")
            .unwrap();
        let row = storage
            .get_nvapi_baseline(game_id.as_str(), key)
            .unwrap()
            .unwrap();
        assert_eq!(row.predefined_dword, None);
        assert!(!row.baseline_was_predefined);
    }

    #[test]
    fn deleting_game_cascades_to_nvapi_rows() {
        let (storage, _path, game_id) = fresh_storage();
        let key = "dlss_sr_render_preset";

        storage
            .upsert_nvapi_executable_override(
                game_id.as_str(),
                "C:/Games/Test/Game.exe",
                "Game.exe",
            )
            .unwrap();
        storage
            .capture_nvapi_baseline_if_missing(game_id.as_str(), key, 0, true, Some(0), "Game.exe")
            .unwrap();

        storage.delete_game(&game_id).expect("delete game");

        assert!(storage
            .get_nvapi_executable_override(game_id.as_str())
            .unwrap()
            .is_none());
        assert!(storage
            .get_nvapi_baseline(game_id.as_str(), key)
            .unwrap()
            .is_none());
    }
}
