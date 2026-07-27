use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

const SQL: &str = r#"
CREATE TABLE IF NOT EXISTS profile_addon_capabilities (
    game_id         TEXT    NOT NULL,
    addon_kind      TEXT    NOT NULL,
    source_revision TEXT    NOT NULL,
    updated_at      INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(addon_kind)) > 0),
    CHECK (length(trim(source_revision)) > 0),
    CHECK (updated_at >= 0),
    PRIMARY KEY (game_id, addon_kind),
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE
) STRICT;
CREATE INDEX IF NOT EXISTS idx_profile_addon_capabilities_kind
    ON profile_addon_capabilities(addon_kind);
CREATE TRIGGER IF NOT EXISTS trg_profile_addon_capabilities_touch_updated_at
AFTER UPDATE ON profile_addon_capabilities
FOR EACH ROW WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE profile_addon_capabilities
       SET updated_at = max(CAST(unixepoch('subsec') * 1000 AS INTEGER), OLD.updated_at + 1)
     WHERE game_id = NEW.game_id AND addon_kind = NEW.addon_kind;
END;
"#;

pub(super) const fn baseline_sql() -> &'static str {
    SQL
}

pub(crate) fn apply(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(SQL)
        .map_err(|error| storage_context("could not create profile add-on capability cache", error))
}
