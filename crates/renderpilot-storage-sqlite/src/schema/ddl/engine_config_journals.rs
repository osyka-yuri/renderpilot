//! Canonical Engine.ini ownership is independent from local add-on installation.

pub(in crate::schema) const SQL: &str = r#"
CREATE TABLE IF NOT EXISTS game_engine_config_journals (
    game_id       TEXT NOT NULL,
    addon_kind    TEXT NOT NULL,
    journal_json  TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,

    PRIMARY KEY (game_id, addon_kind),
    UNIQUE (game_id),
    CHECK (length(trim(game_id)) > 0),
    CHECK (addon_kind IN ('renodx', 'luma')),
    CHECK (json_valid(journal_json) AND json_type(journal_json) = 'object'),
    CHECK (created_at >= 0),
    CHECK (updated_at >= created_at)
) STRICT;
"#;
