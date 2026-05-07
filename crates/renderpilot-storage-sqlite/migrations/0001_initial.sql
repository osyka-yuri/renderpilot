PRAGMA foreign_keys = ON;

-- =============================================================================
-- RenderPilot catalog schema
--
-- Notes:
-- - Timestamps are stored as Unix milliseconds.
-- - Paths are stored as normalized PathRef-style UTF-8 strings with `/` separators.
-- - JSON fields are stored as TEXT and validated with json_valid(...).
-- - Tables use STRICT mode to avoid accidental SQLite type coercion.
-- =============================================================================


-- =============================================================================
-- Games
-- =============================================================================

CREATE TABLE IF NOT EXISTS games (
    id                         TEXT    PRIMARY KEY NOT NULL,
    title                      TEXT    NOT NULL,
    launcher                   TEXT    NOT NULL,
    external_id                TEXT,
    platform                   TEXT    NOT NULL,
    runtime                    TEXT    NOT NULL,
    install_path               TEXT    NOT NULL,
    executable_candidates_json TEXT    NOT NULL,
    updated_at                 INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),

    CHECK (length(trim(id)) > 0),
    CHECK (length(trim(title)) > 0),
    CHECK (length(trim(launcher)) > 0),
    CHECK (external_id IS NULL OR length(trim(external_id)) > 0),
    CHECK (length(trim(platform)) > 0),
    CHECK (length(trim(runtime)) > 0),
    CHECK (length(trim(install_path)) > 0),
    CHECK (instr(install_path, char(0)) = 0),
    CHECK (json_valid(executable_candidates_json)),
    CHECK (updated_at >= 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_games_launcher_install_path
    ON games(launcher, install_path);

CREATE INDEX IF NOT EXISTS idx_games_updated_at
    ON games(updated_at);


-- =============================================================================
-- Components
-- =============================================================================

CREATE TABLE IF NOT EXISTS components (
    id           TEXT    PRIMARY KEY NOT NULL,
    game_id      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    technology   TEXT    NOT NULL,
    swappability TEXT    NOT NULL,
    files_json   TEXT    NOT NULL,
    updated_at   INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),

    FOREIGN KEY (game_id)
        REFERENCES games(id)
        ON DELETE CASCADE,

    CHECK (length(trim(id)) > 0),
    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(kind)) > 0),
    CHECK (length(trim(technology)) > 0),
    CHECK (length(trim(swappability)) > 0),
    CHECK (json_valid(files_json)),
    CHECK (updated_at >= 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_components_game_id
    ON components(game_id);

CREATE INDEX IF NOT EXISTS idx_components_game_id_technology
    ON components(game_id, technology);

CREATE INDEX IF NOT EXISTS idx_components_technology
    ON components(technology);


-- =============================================================================
-- Library artifacts
-- =============================================================================

CREATE TABLE IF NOT EXISTS library_artifacts (
    id             TEXT    PRIMARY KEY NOT NULL,
    technology     TEXT    NOT NULL,
    file_name      TEXT    NOT NULL,
    file_path      TEXT    NOT NULL,
    version        TEXT,
    sha256         TEXT    NOT NULL UNIQUE,
    source         TEXT,
    source_game_id TEXT,
    trust_level    TEXT    NOT NULL,
    updated_at     INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),

    FOREIGN KEY (source_game_id)
        REFERENCES games(id)
        ON DELETE SET NULL,

    CHECK (length(trim(id)) > 0),
    CHECK (length(trim(technology)) > 0),
    CHECK (length(trim(file_name)) > 0),
    CHECK (length(trim(file_path)) > 0),
    CHECK (instr(file_path, char(0)) = 0),
    CHECK (version IS NULL OR length(trim(version)) > 0),
    CHECK (length(sha256) = 64),
    CHECK (sha256 NOT GLOB '*[^0-9a-f]*'),
    CHECK (source IS NULL OR length(trim(source)) > 0),
    CHECK (source_game_id IS NULL OR length(trim(source_game_id)) > 0),
    CHECK (length(trim(trust_level)) > 0),
    CHECK (updated_at >= 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_library_artifacts_technology
    ON library_artifacts(technology);

CREATE INDEX IF NOT EXISTS idx_library_artifacts_technology_file_name
    ON library_artifacts(technology, file_name);

CREATE INDEX IF NOT EXISTS idx_library_artifacts_sha256
    ON library_artifacts(sha256);

CREATE INDEX IF NOT EXISTS idx_library_artifacts_source_game_id
    ON library_artifacts(source_game_id);

CREATE INDEX IF NOT EXISTS idx_library_artifacts_updated_at
    ON library_artifacts(updated_at);


-- =============================================================================
-- Operations
-- =============================================================================

CREATE TABLE IF NOT EXISTS operations (
    id            TEXT    PRIMARY KEY NOT NULL,
    game_id       TEXT    NOT NULL,
    kind          TEXT    NOT NULL,
    status        TEXT    NOT NULL,
    created_at    INTEGER NOT NULL,
    completed_at  INTEGER,
    metadata_json TEXT,

    FOREIGN KEY (game_id)
        REFERENCES games(id)
        ON DELETE CASCADE,

    CHECK (length(trim(id)) > 0),
    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(kind)) > 0),
    CHECK (length(trim(status)) > 0),
    CHECK (created_at >= 0),
    CHECK (completed_at IS NULL OR completed_at >= 0),
    CHECK (completed_at IS NULL OR completed_at >= created_at),
    CHECK (metadata_json IS NULL OR json_valid(metadata_json))
) STRICT;

CREATE INDEX IF NOT EXISTS idx_operations_game_id
    ON operations(game_id);

CREATE INDEX IF NOT EXISTS idx_operations_game_id_created_at
    ON operations(game_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_operations_status
    ON operations(status);


-- =============================================================================
-- Operation items
-- =============================================================================

CREATE TABLE IF NOT EXISTS operation_items (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id  TEXT    NOT NULL,
    component_id  TEXT    NOT NULL,
    artifact_id   TEXT,
    source_path   TEXT    NOT NULL,
    target_path   TEXT,
    status        TEXT    NOT NULL,
    metadata_json TEXT,

    FOREIGN KEY (operation_id)
        REFERENCES operations(id)
        ON DELETE CASCADE,

    FOREIGN KEY (component_id)
        REFERENCES components(id)
        ON DELETE CASCADE,

    FOREIGN KEY (artifact_id)
        REFERENCES library_artifacts(id)
        ON DELETE SET NULL,

    CHECK (length(trim(operation_id)) > 0),
    CHECK (length(trim(component_id)) > 0),
    CHECK (artifact_id IS NULL OR length(trim(artifact_id)) > 0),
    CHECK (length(trim(source_path)) > 0),
    CHECK (instr(source_path, char(0)) = 0),
    CHECK (target_path IS NULL OR length(trim(target_path)) > 0),
    CHECK (target_path IS NULL OR instr(target_path, char(0)) = 0),
    CHECK (length(trim(status)) > 0),
    CHECK (metadata_json IS NULL OR json_valid(metadata_json))
) STRICT;

CREATE INDEX IF NOT EXISTS idx_operation_items_operation_id
    ON operation_items(operation_id);

CREATE INDEX IF NOT EXISTS idx_operation_items_component_id
    ON operation_items(component_id);

CREATE INDEX IF NOT EXISTS idx_operation_items_artifact_id
    ON operation_items(artifact_id);

CREATE INDEX IF NOT EXISTS idx_operation_items_status
    ON operation_items(status);


-- =============================================================================
-- Backups
-- =============================================================================

CREATE TABLE IF NOT EXISTS backups (
    id            TEXT    PRIMARY KEY NOT NULL,
    operation_id  TEXT    NOT NULL,
    game_id       TEXT    NOT NULL,
    component_id  TEXT,
    original_path TEXT    NOT NULL,
    backup_path   TEXT    NOT NULL,
    sha256        TEXT,
    created_at    INTEGER NOT NULL,
    metadata_json TEXT,

    FOREIGN KEY (operation_id)
        REFERENCES operations(id)
        ON DELETE CASCADE,

    FOREIGN KEY (game_id)
        REFERENCES games(id)
        ON DELETE CASCADE,

    FOREIGN KEY (component_id)
        REFERENCES components(id)
        ON DELETE SET NULL,

    CHECK (length(trim(id)) > 0),
    CHECK (length(trim(operation_id)) > 0),
    CHECK (length(trim(game_id)) > 0),
    CHECK (component_id IS NULL OR length(trim(component_id)) > 0),
    CHECK (length(trim(original_path)) > 0),
    CHECK (length(trim(backup_path)) > 0),
    CHECK (instr(original_path, char(0)) = 0),
    CHECK (instr(backup_path, char(0)) = 0),
    CHECK (sha256 IS NULL OR length(sha256) = 64),
    CHECK (sha256 IS NULL OR sha256 NOT GLOB '*[^0-9a-f]*'),
    CHECK (created_at >= 0),
    CHECK (metadata_json IS NULL OR json_valid(metadata_json))
) STRICT;

CREATE INDEX IF NOT EXISTS idx_backups_operation_id
    ON backups(operation_id);

CREATE INDEX IF NOT EXISTS idx_backups_game_id
    ON backups(game_id);

CREATE INDEX IF NOT EXISTS idx_backups_component_id
    ON backups(component_id);

CREATE INDEX IF NOT EXISTS idx_backups_created_at
    ON backups(created_at DESC);


-- =============================================================================
-- Settings
-- =============================================================================

CREATE TABLE IF NOT EXISTS settings (
    key        TEXT    PRIMARY KEY NOT NULL,
    value      TEXT    NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),

    CHECK (length(trim(key)) > 0),
    CHECK (length(trim(value)) > 0),
    CHECK (updated_at >= 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_settings_updated_at
    ON settings(updated_at);


-- =============================================================================
-- File hash cache
--
-- Per-path SHA-256 cache.
-- Used to skip re-hashing and PE-version extraction when size and mtime match.
-- =============================================================================

CREATE TABLE IF NOT EXISTS file_hash_cache (
    path        TEXT    PRIMARY KEY NOT NULL,
    size        INTEGER NOT NULL,
    modified_at INTEGER NOT NULL,
    sha256      TEXT    NOT NULL,
    version     TEXT,
    updated_at  INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),

    CHECK (length(trim(path)) > 0),
    CHECK (instr(path, char(0)) = 0),
    CHECK (size >= 0),
    CHECK (modified_at >= 0),
    CHECK (length(sha256) = 64),
    CHECK (sha256 NOT GLOB '*[^0-9a-f]*'),
    CHECK (version IS NULL OR length(trim(version)) > 0),
    CHECK (updated_at >= 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_file_hash_cache_updated_at
    ON file_hash_cache(updated_at);