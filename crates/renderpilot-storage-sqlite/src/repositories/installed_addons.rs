//! Persistence for installed add-on records (initially RenoDX).
//!
//! One row per game records everything needed to reverse an install: the files
//! RenderPilot created and the pre-existing files it backed up. This is the
//! source of truth for uninstall, so the table intentionally has no foreign
//! key to `games` and survives catalog pruning/rescans.

use renderpilot_application::AppResult;
use renderpilot_application::InstalledAddonRepository;
#[cfg(test)]
use renderpilot_domain::TrackedSourceRole;
use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, PathRef, TrackedSource,
};
use rusqlite::{OptionalExtension, Row, named_params};

use crate::error::{invalid_row, storage_error};
use crate::{mapping, sqlite_clock};

use super::SqliteStorage;

const UPSERT_SQL: &str = "
    INSERT INTO installed_addons
        (game_id, kind, addon_file, addon_version,
         created_files_json, backed_up_files_json, tracked_sources_json,
         host_kind, reshade_channel, registered_exe_path,
         created_at, updated_at)
    VALUES
        (:game_id, :kind, :addon_file, :addon_version,
         :created_files, :backed_up_files, :tracked_sources,
         :host_kind, :reshade_channel, :registered_exe_path,
         :now_ms, :now_ms)
    ON CONFLICT(game_id) DO UPDATE SET
        kind                 = excluded.kind,
        addon_file           = excluded.addon_file,
        addon_version        = excluded.addon_version,
        created_files_json   = excluded.created_files_json,
        backed_up_files_json = excluded.backed_up_files_json,
        tracked_sources_json = excluded.tracked_sources_json,
        host_kind            = excluded.host_kind,
        reshade_channel      = excluded.reshade_channel,
        registered_exe_path  = excluded.registered_exe_path,
        updated_at           = excluded.updated_at
";

const GET_SQL: &str = "
    SELECT game_id, kind, addon_file, addon_version,
           created_files_json, backed_up_files_json, tracked_sources_json,
           host_kind, reshade_channel, registered_exe_path,
           created_at, updated_at
    FROM installed_addons
    WHERE game_id = :game_id
";

const LIST_SQL: &str = "
    SELECT game_id, kind, addon_file, addon_version,
           created_files_json, backed_up_files_json, tracked_sources_json,
           host_kind, reshade_channel, registered_exe_path,
           created_at, updated_at
    FROM installed_addons
    ORDER BY game_id
";

impl InstalledAddonRepository for SqliteStorage {
    fn upsert_installed_addon(&self, addon: &InstalledAddon) -> AppResult<()> {
        self.with_transaction(|transaction| {
            let now_ms = sqlite_clock::now_ms(transaction)?;
            let host_kind = addon
                .host_kind()
                .map(|kind| mapping::enum_to_text(&kind))
                .transpose()?;
            transaction
                .prepare_cached(UPSERT_SQL)
                .map_err(storage_error)?
                .execute(named_params! {
                    ":game_id": addon.game_id().as_str(),
                    ":kind": mapping::enum_to_text(&addon.kind())?,
                    ":addon_file": addon.addon_file().as_str(),
                    ":addon_version": addon.addon_version(),
                    ":created_files": mapping::serialize_json(addon.created_files())?,
                    ":backed_up_files": mapping::serialize_json(addon.backed_up_files())?,
                    ":tracked_sources": mapping::serialize_json(addon.tracked_sources())?,
                    ":host_kind": host_kind.as_deref(),
                    ":reshade_channel": addon.reshade_channel(),
                    ":registered_exe_path": addon.registered_exe_path().map(PathRef::as_str),
                    ":now_ms": now_ms,
                })
                .map_err(storage_error)?;
            Ok(())
        })
    }

    fn get_installed_addon(&self, game_id: &GameId) -> AppResult<Option<InstalledAddon>> {
        self.with_connection(|connection| {
            connection
                .prepare_cached(GET_SQL)
                .map_err(storage_error)?
                .query_row(named_params! { ":game_id": game_id.as_str() }, |row| {
                    Ok(row_to_installed_addon(row))
                })
                .optional()
                .map_err(storage_error)?
                .transpose()
        })
    }

    fn list_installed_addons(&self) -> AppResult<Vec<InstalledAddon>> {
        self.query_list(LIST_SQL, [], |row| Ok(row_to_installed_addon(row)))
    }

    fn delete_installed_addon(&self, game_id: &GameId) -> AppResult<()> {
        self.with_connection(|connection| {
            connection
                .prepare_cached("DELETE FROM installed_addons WHERE game_id = :game_id")
                .map_err(storage_error)?
                .execute(named_params! { ":game_id": game_id.as_str() })
                .map_err(storage_error)?;
            Ok(())
        })
    }
}

/// Maps a result row (selected by [`GET_SQL`]/[`LIST_SQL`]) to an [`InstalledAddon`].
///
/// Columns are read by name so the mapping cannot silently drift if the column
/// order in the queries ever changes. The outer `rusqlite::Result` carries
/// column-extraction errors; the inner `AppResult` carries domain
/// parsing/validation errors.
fn row_to_installed_addon(row: &Row<'_>) -> AppResult<InstalledAddon> {
    let game_id = GameId::new(row.get::<_, String>("game_id").map_err(storage_error)?)
        .map_err(invalid_row)?;
    let kind: AddonKind =
        mapping::enum_from_text(&row.get::<_, String>("kind").map_err(storage_error)?)?;
    let addon_file = PathRef::new(row.get::<_, String>("addon_file").map_err(storage_error)?)
        .map_err(invalid_row)?;
    let addon_version: Option<String> = row.get("addon_version").map_err(storage_error)?;
    let created_files: Vec<PathRef> = mapping::deserialize_json(
        &row.get::<_, String>("created_files_json")
            .map_err(storage_error)?,
    )?;
    let backed_up_files: Vec<PathRef> = mapping::deserialize_json(
        &row.get::<_, String>("backed_up_files_json")
            .map_err(storage_error)?,
    )?;
    let tracked_sources: Vec<TrackedSource> = mapping::deserialize_json(
        &row.get::<_, String>("tracked_sources_json")
            .map_err(storage_error)?,
    )?;
    let host_kind = row
        .get::<_, Option<String>>("host_kind")
        .map_err(storage_error)?
        .map(|value| mapping::enum_from_text::<InstalledAddonHostKind>(&value))
        .transpose()?;
    let reshade_channel: Option<String> = row.get("reshade_channel").map_err(storage_error)?;
    let registered_exe_path: Option<PathRef> = row
        .get::<_, Option<String>>("registered_exe_path")
        .map_err(storage_error)?
        .map(PathRef::new)
        .transpose()
        .map_err(invalid_row)?;
    let created_at: i64 = row.get("created_at").map_err(storage_error)?;
    let updated_at: i64 = row.get("updated_at").map_err(storage_error)?;

    let mut record = InstalledAddon::from_parts(
        game_id,
        kind,
        addon_file,
        addon_version,
        created_files,
        backed_up_files,
        tracked_sources,
    )
    .ok_or_else(|| invalid_row("created_files must contain addon_file"))?
    .with_timestamps(Some(created_at), Some(updated_at));

    if let Some(host_kind) = host_kind {
        record = record.with_host_kind(host_kind);
    }
    if let Some(channel) = reshade_channel {
        record = record.with_reshade_channel(channel);
    }
    if let Some(path) = registered_exe_path {
        record = record.with_registered_exe_path(path);
    }

    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_id() -> GameId {
        GameId::new("steam:1091500").expect("id")
    }

    fn path(value: &str) -> PathRef {
        PathRef::new(value).expect("path")
    }

    fn recorded_host_addon() -> InstalledAddon {
        InstalledAddon::new(
            game_id(),
            AddonKind::RenoDx,
            path("C:/Games/CP2077/renodx-cp2077.addon64"),
        )
        .with_addon_version("snapshot-2026.06")
        .with_created_file(path("C:/Games/CP2077/dxgi.dll"))
        .with_created_file(path("C:/Games/CP2077/ReShade.ini"))
        .with_tracked_source(
            TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                "https://clshortfuse.github.io/renodx/renodx-cp2077.addon64",
                Some("\"etag-1\"".to_owned()),
                "addon-digest",
            )
            .with_last_modified(Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned())),
        )
        .with_tracked_source(TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://nightly.link/x64.zip",
            None,
            "host-digest",
        ))
    }

    #[test]
    fn upsert_then_get_round_trips_a_recorded_host_install() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let addon = recorded_host_addon();
        storage.upsert_installed_addon(&addon).expect("upsert");

        let loaded = storage
            .get_installed_addon(&game_id())
            .expect("get")
            .expect("present");

        assert_eq!(loaded.addon_version(), Some("snapshot-2026.06"));
        assert!(loaded.has_host_binary_provenance());
        assert_eq!(loaded.addon_file().as_str(), addon.addon_file().as_str());
        assert_eq!(loaded.created_files(), addon.created_files());
        assert_eq!(loaded.backed_up_files(), addon.backed_up_files());
        assert_eq!(loaded.tracked_sources(), addon.tracked_sources());
        // The persisted upstream date + install/update timestamps surface on read.
        assert_eq!(loaded.addon_dated(), Some("Wed, 18 Jun 2026 12:00:00 GMT"));
        assert!(loaded.installed_at().is_some());
        assert!(loaded.updated_at().is_some());
    }

    #[test]
    fn upsert_then_get_round_trips_host_metadata() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let addon = recorded_host_addon()
            .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer)
            .with_reshade_channel("nightly")
            .with_registered_exe_path(path("C:/Games/CP2077/bin/x64/Cyberpunk2077.exe"));

        storage.upsert_installed_addon(&addon).expect("upsert");
        let loaded = storage
            .get_installed_addon(&game_id())
            .expect("get")
            .expect("present");

        assert_eq!(
            loaded.host_kind(),
            Some(InstalledAddonHostKind::SharedVulkanLayer)
        );
        assert_eq!(loaded.reshade_channel(), Some("nightly"));
        assert_eq!(
            loaded.registered_exe_path().map(PathRef::as_str),
            Some("C:/Games/CP2077/bin/x64/Cyberpunk2077.exe")
        );
    }

    #[test]
    fn upsert_replaces_an_existing_record() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage
            .upsert_installed_addon(&recorded_host_addon())
            .expect("first");

        // A local-file install records no HostBinary entry.
        let local_file = InstalledAddon::new(
            game_id(),
            AddonKind::RenoDx,
            path("C:/Games/CP2077/renodx-cp2077.addon64"),
        )
        .with_backed_up_file(path("C:/Games/CP2077/ReShade.ini"));
        storage.upsert_installed_addon(&local_file).expect("second");

        let loaded = storage
            .get_installed_addon(&game_id())
            .expect("get")
            .expect("present");
        assert!(!loaded.has_host_binary_provenance());
        assert!(loaded.tracked_sources().is_empty());
        assert_eq!(loaded.backed_up_files().len(), 1);
    }

    #[test]
    fn get_returns_none_when_absent() {
        let storage = SqliteStorage::in_memory().expect("storage");
        assert!(
            storage
                .get_installed_addon(&game_id())
                .expect("get")
                .is_none()
        );
    }

    #[test]
    fn delete_removes_the_record() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage
            .upsert_installed_addon(&recorded_host_addon())
            .expect("upsert");
        storage.delete_installed_addon(&game_id()).expect("delete");
        assert!(
            storage
                .get_installed_addon(&game_id())
                .expect("get")
                .is_none()
        );
    }

    #[test]
    fn list_returns_all_records() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage
            .upsert_installed_addon(&recorded_host_addon())
            .expect("upsert");
        assert_eq!(storage.list_installed_addons().expect("list").len(), 1);
    }
}
