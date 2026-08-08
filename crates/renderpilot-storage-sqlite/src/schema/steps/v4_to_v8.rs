//! Add RenoDX installation tracking to the released v4 catalog.
//!
//! Schema versions 5 through 7 were never released. The application v1.3
//! baseline squashed their development iterations into the v4→v8 edge below.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

use super::super::version;

pub(super) const SOURCE_VERSION: i32 = 4;
pub(super) const TARGET_VERSION: i32 = 8;

const CREATE_INSTALLED_ADDONS: &str = "
    CREATE TABLE IF NOT EXISTS installed_addons (
        game_id               TEXT    PRIMARY KEY NOT NULL,
        kind                  TEXT    NOT NULL,
        addon_file            TEXT    NOT NULL,
        addon_version         TEXT,
        created_files_json    TEXT    NOT NULL,
        backed_up_files_json  TEXT    NOT NULL,
        tracked_sources_json  TEXT    NOT NULL DEFAULT '[]',
        created_at            INTEGER NOT NULL DEFAULT (
            CAST(unixepoch('subsec') * 1000 AS INTEGER)
        ),
        updated_at            INTEGER NOT NULL DEFAULT (
            CAST(unixepoch('subsec') * 1000 AS INTEGER)
        ),

        CHECK (length(trim(game_id)) > 0),
        CHECK (length(trim(kind)) > 0),
        CHECK (length(trim(addon_file)) > 0),
        CHECK (json_valid(created_files_json)),
        CHECK (json_type(created_files_json) = 'array'),
        CHECK (json_valid(backed_up_files_json)),
        CHECK (json_type(backed_up_files_json) = 'array'),
        CHECK (json_valid(tracked_sources_json)),
        CHECK (json_type(tracked_sources_json) = 'array'),
        CHECK (created_at >= 0),
        CHECK (updated_at >= created_at)
    ) STRICT;

    CREATE TRIGGER IF NOT EXISTS trg_installed_addons_touch_updated_at
    AFTER UPDATE ON installed_addons
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
    BEGIN
        UPDATE installed_addons
           SET updated_at = max(
               CAST(unixepoch('subsec') * 1000 AS INTEGER),
               OLD.updated_at + 1
           )
         WHERE game_id = NEW.game_id;
    END;
";

pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(CREATE_INSTALLED_ADDONS)
        .map_err(|error| storage_context("could not create v8 installed add-on schema", error))?;
    version::write(connection, TARGET_VERSION)
}
