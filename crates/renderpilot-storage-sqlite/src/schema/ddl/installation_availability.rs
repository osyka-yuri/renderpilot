//! Current installed/absent projection for raw registered installation rows.

pub(in crate::schema) const SQL: &str = r#"
CREATE TABLE IF NOT EXISTS installation_availability (
    game_id     TEXT PRIMARY KEY NOT NULL,
    state       TEXT NOT NULL,
    revision    INTEGER NOT NULL DEFAULT 0,
    updated_at  INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),

    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE,
    CHECK (length(trim(game_id)) > 0),
    CHECK (state IN ('active', 'absent')),
    CHECK (revision >= 0),
    CHECK (updated_at >= 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_installation_availability_state
    ON installation_availability(state, game_id);

CREATE TRIGGER IF NOT EXISTS trg_games_create_installation_availability
AFTER INSERT ON games
FOR EACH ROW
BEGIN
    INSERT OR IGNORE INTO installation_availability (game_id, state, revision)
    VALUES (NEW.id, 'active', 0);
END;
"#;
