//! Persistence for installed add-on records (initially RenoDX).
//!
//! One row per game records everything needed to reverse an install: the files
//! RenderPilot created and the pre-existing files it backed up. This is the
//! source of truth for uninstall, so the table intentionally has no foreign
//! key to `games` and survives catalog pruning/rescans.

use renderpilot_application::AppResult;
use renderpilot_application::{AppError, InstalledAddonRepository};
#[cfg(test)]
use renderpilot_domain::TrackedSourceRole;
use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, InstalledAddonParts,
    ManagedAddonFile, PathRef, TrackedSource,
};
use rusqlite::{OptionalExtension, Row, Transaction, named_params};

use crate::error::{invalid_row, storage_error};
use crate::{mapping, sqlite_clock};

use super::SqliteStorage;

const EXISTING_KIND_SQL: &str = "SELECT kind FROM installed_addons WHERE game_id = :game_id";

const UPSERT_SQL: &str = "
    INSERT INTO installed_addons
        (game_id, kind, addon_file, addon_version,
         created_files_json, backed_up_files_json, managed_files_json, tracked_sources_json,
         host_kind, reshade_channel, registered_exe_path,
         created_at, updated_at)
    VALUES
        (:game_id, :kind, :addon_file, :addon_version,
         :created_files, :backed_up_files, :managed_files, :tracked_sources,
         :host_kind, :reshade_channel, :registered_exe_path,
         :now_ms, :now_ms)
    ON CONFLICT(game_id) DO UPDATE SET
        kind                 = excluded.kind,
        addon_file           = excluded.addon_file,
        addon_version        = excluded.addon_version,
        created_files_json   = excluded.created_files_json,
        backed_up_files_json = excluded.backed_up_files_json,
        managed_files_json   = excluded.managed_files_json,
        tracked_sources_json = excluded.tracked_sources_json,
        host_kind            = excluded.host_kind,
        reshade_channel      = excluded.reshade_channel,
        registered_exe_path  = excluded.registered_exe_path,
        updated_at           = excluded.updated_at
";

const GET_SQL: &str = "
    SELECT game_id, kind, addon_file, addon_version,
           created_files_json, backed_up_files_json, managed_files_json, tracked_sources_json,
           host_kind, reshade_channel, registered_exe_path,
           created_at, updated_at
    FROM installed_addons
    WHERE game_id = :game_id
";

const LIST_SQL: &str = "
    SELECT game_id, kind, addon_file, addon_version,
           created_files_json, backed_up_files_json, managed_files_json, tracked_sources_json,
           host_kind, reshade_channel, registered_exe_path,
           created_at, updated_at
    FROM installed_addons
    ORDER BY game_id
";

const DELETE_SQL: &str = "DELETE FROM installed_addons WHERE game_id = :game_id";

impl InstalledAddonRepository for SqliteStorage {
    fn upsert_installed_addon(&self, addon: &InstalledAddon) -> AppResult<()> {
        self.with_transaction(|transaction| upsert_within_transaction(transaction, addon))
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

    fn delete_installed_addon(&self, game_id: &GameId, kind: AddonKind) -> AppResult<()> {
        self.with_transaction(|transaction| delete_within_transaction(transaction, game_id, kind))
    }
}

pub(super) fn upsert_within_transaction(
    transaction: &Transaction<'_>,
    addon: &InstalledAddon,
) -> AppResult<()> {
    let existing_kind: Option<String> = transaction
        .prepare_cached(EXISTING_KIND_SQL)
        .map_err(storage_error)?
        .query_row(
            named_params! { ":game_id": addon.game_id().as_str() },
            |row| row.get("kind"),
        )
        .optional()
        .map_err(storage_error)?;
    let new_kind = addon.kind().as_str();
    if let Some(existing_kind) = &existing_kind
        && existing_kind != new_kind
    {
        return Err(AppError::invalid_input(format!(
            "refusing to overwrite a '{existing_kind}' install record with a \
             '{new_kind}' one for {}; uninstall it first",
            addon.game_id()
        )));
    }

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
            ":managed_files": mapping::serialize_json(addon.managed_files())?,
            ":tracked_sources": mapping::serialize_json(addon.tracked_sources())?,
            ":host_kind": host_kind.as_deref(),
            ":reshade_channel": addon.reshade_channel(),
            ":registered_exe_path": addon.registered_exe_path().map(PathRef::as_str),
            ":now_ms": now_ms,
        })
        .map_err(storage_error)?;
    Ok(())
}

pub(super) fn delete_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    kind: AddonKind,
) -> AppResult<()> {
    let existing_kind: Option<String> = transaction
        .prepare_cached(EXISTING_KIND_SQL)
        .map_err(storage_error)?
        .query_row(named_params! { ":game_id": game_id.as_str() }, |row| {
            row.get("kind")
        })
        .optional()
        .map_err(storage_error)?;
    let expected_kind = kind.as_str();
    if let Some(existing_kind) = &existing_kind
        && existing_kind != expected_kind
    {
        return Err(AppError::invalid_input(format!(
            "refusing to delete a '{existing_kind}' install record as a \
             '{expected_kind}' one for {}; uninstall it with the correct kind",
            game_id
        )));
    }

    transaction
        .prepare_cached(DELETE_SQL)
        .map_err(storage_error)?
        .execute(named_params! {
            ":game_id": game_id.as_str(),
        })
        .map_err(storage_error)?;
    Ok(())
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
    let managed_files: Vec<ManagedAddonFile> = mapping::deserialize_json(
        &row.get::<_, String>("managed_files_json")
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

    let mut record = InstalledAddon::from_parts_with_managed(InstalledAddonParts {
        game_id,
        kind,
        addon_file,
        addon_version,
        created_files,
        backed_up_files,
        managed_files,
        tracked_sources,
    })
    .map_err(invalid_row)?
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
    fn corrupt_reused_managed_binding_is_rejected_on_rehydrate() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let record = InstalledAddon::new(
            game_id(),
            AddonKind::Luma,
            path("C:/Games/CP2077/Luma-Test.addon64"),
        );
        storage.upsert_installed_addon(&record).expect("record");
        storage
            .connection
            .lock()
            .expect("connection")
            .execute(
                "UPDATE installed_addons SET managed_files_json = ?1 WHERE game_id = ?2",
                rusqlite::params![
                    r#"[{"path":"C:/Games/CP2077/nvngx_dlss.dll","mode":"reused","baseline":{"state":"absent"},"installed_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]"#,
                    game_id().as_str()
                ],
            )
            .expect("corrupt row");

        assert!(storage.get_installed_addon(&game_id()).is_err());
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

    /// E.17: a Luma record's `created_files` are deep, multi-component paths
    /// (the `Luma/**` shader tree) rather than RenoDX's flat, single-file
    /// layout — the JSON round-trip must preserve them exactly.
    #[test]
    fn upsert_then_get_round_trips_a_luma_record_with_nested_created_files() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let addon = InstalledAddon::new(
            game_id(),
            AddonKind::Luma,
            path("C:/Games/Dishonored2/Luma-Dishonored_2.addon"),
        )
        .with_addon_version("Build 515")
        .with_created_file(path("C:/Games/Dishonored2/dxgi.dll"))
        .with_created_file(path("C:/Games/Dishonored2/Luma/Global/Copy_PS.hlsl"))
        .with_created_file(path("C:/Games/Dishonored2/Luma/Includes/Common.hlsl"))
        .with_created_file(path(
            "C:/Games/Dishonored2/Luma/Dishonored 2/Fog_PS.hlsl",
        ))
        .with_tracked_source(TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://github.com/Filoppi/Luma-Framework/releases/latest/download/Luma-Dishonored_2.zip",
            None,
            "zip-digest",
        ));

        storage.upsert_installed_addon(&addon).expect("upsert");
        let loaded = storage
            .get_installed_addon(&game_id())
            .expect("get")
            .expect("present");

        assert_eq!(loaded.kind(), AddonKind::Luma);
        assert_eq!(loaded.created_files(), addon.created_files());
        assert!(
            loaded
                .created_files()
                .iter()
                .any(|p| p.as_str().ends_with("Luma/Dishonored 2/Fog_PS.hlsl")),
            "a deeply nested path with a space in a component must round-trip"
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
        storage
            .delete_installed_addon(&game_id(), AddonKind::RenoDx)
            .expect("delete");
        assert!(
            storage
                .get_installed_addon(&game_id())
                .expect("get")
                .is_none()
        );
    }

    #[test]
    fn delete_with_wrong_kind_is_rejected() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage
            .upsert_installed_addon(&recorded_host_addon())
            .expect("upsert");
        let error = storage
            .delete_installed_addon(&game_id(), AddonKind::Luma)
            .expect_err("wrong-kind delete must be refused");
        assert!(error.message().contains("renodx"));
        assert!(error.message().contains("luma"));

        // The original RenoDX record must be untouched.
        let loaded = storage
            .get_installed_addon(&game_id())
            .expect("get")
            .expect("renodx row must survive");
        assert_eq!(loaded.kind(), AddonKind::RenoDx);
    }

    #[test]
    fn list_returns_all_records() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage
            .upsert_installed_addon(&recorded_host_addon())
            .expect("upsert");
        assert_eq!(storage.list_installed_addons().expect("list").len(), 1);
    }

    #[test]
    fn upsert_replaces_a_same_kind_record_without_a_guard_error() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage
            .upsert_installed_addon(&recorded_host_addon())
            .expect("first insert");

        let updated = InstalledAddon::new(
            game_id(),
            AddonKind::RenoDx,
            path("C:/Games/CP2077/renodx-cp2077.addon64"),
        )
        .with_addon_version("snapshot-2026.07");
        storage
            .upsert_installed_addon(&updated)
            .expect("same-kind replace");

        let loaded = storage
            .get_installed_addon(&game_id())
            .expect("get")
            .expect("present");
        assert_eq!(loaded.addon_version(), Some("snapshot-2026.07"));
    }

    #[test]
    fn upsert_refuses_to_overwrite_a_different_kind_record() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage
            .upsert_installed_addon(&recorded_host_addon())
            .expect("renodx insert");

        let luma_record = InstalledAddon::new(
            game_id(),
            AddonKind::Luma,
            path("C:/Games/CP2077/Luma-Game.addon"),
        );
        let error = storage
            .upsert_installed_addon(&luma_record)
            .expect_err("cross-kind upsert must be refused");
        assert!(error.message().contains("renodx"));
        assert!(error.message().contains("luma"));

        // The original RenoDX record must be untouched.
        let loaded = storage
            .get_installed_addon(&game_id())
            .expect("get")
            .expect("present");
        assert_eq!(loaded.kind(), AddonKind::RenoDx);
    }

    #[test]
    fn upsert_allows_a_different_kind_after_the_prior_record_is_deleted() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage
            .upsert_installed_addon(&recorded_host_addon())
            .expect("renodx insert");
        storage
            .delete_installed_addon(&game_id(), AddonKind::RenoDx)
            .expect("delete");

        let luma_record = InstalledAddon::new(
            game_id(),
            AddonKind::Luma,
            path("C:/Games/CP2077/Luma-Game.addon"),
        );
        storage
            .upsert_installed_addon(&luma_record)
            .expect("insert after delete");

        let loaded = storage
            .get_installed_addon(&game_id())
            .expect("get")
            .expect("present");
        assert_eq!(loaded.kind(), AddonKind::Luma);
    }
}
