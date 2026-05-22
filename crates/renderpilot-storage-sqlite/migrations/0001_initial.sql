PRAGMA foreign_keys = ON;

-- =============================================================================
-- RenderPilot catalog schema v2
--
-- Notes:
-- - Timestamps are Unix milliseconds.
-- - Paths are normalized PathRef-style UTF-8 strings with "/" separators.
-- - JSON fields are stored as TEXT and validated with json_valid/json_type.
-- - Tables use STRICT mode.
-- - Lifecycle: there is no incremental migration from older on-disk shapes; see `src/schema.rs`
--   (`apply` / `validate_catalog_schema`) and `src/repositories/columns.rs` for the bundled-schema contract.
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
    created_at                 INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
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
    CHECK (instr(install_path, '\') = 0),
    CHECK (json_valid(executable_candidates_json)),
    CHECK (json_type(executable_candidates_json) = 'array'),
    CHECK (created_at >= 0),
    CHECK (updated_at >= created_at)
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS uq_games_launcher_external_id
    ON games(launcher, external_id)
    WHERE external_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_games_launcher_install_path
    ON games(launcher, install_path);

CREATE INDEX IF NOT EXISTS idx_games_updated_at
    ON games(updated_at DESC);


-- =============================================================================
-- Game covers (image files live beside catalog.db under covers/)
-- =============================================================================

CREATE TABLE IF NOT EXISTS game_covers (
    game_id    TEXT    PRIMARY KEY NOT NULL,
    file_name  TEXT    NOT NULL,
    updated_at INTEGER NOT NULL,

    FOREIGN KEY (game_id)
        REFERENCES games(id)
        ON DELETE CASCADE,

    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(file_name)) > 0),
    CHECK (instr(file_name, '/') = 0),
    CHECK (instr(file_name, '\') = 0),
    CHECK (updated_at >= 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_game_covers_updated_at
    ON game_covers(updated_at DESC);


-- =============================================================================
-- Components
-- =============================================================================

CREATE TABLE IF NOT EXISTS components (
    id           TEXT    PRIMARY KEY NOT NULL,
    game_id      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    library      TEXT    NOT NULL,
    swappability TEXT    NOT NULL,
    files_json   TEXT    NOT NULL,
    created_at   INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    updated_at   INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),

    FOREIGN KEY (game_id)
        REFERENCES games(id)
        ON DELETE CASCADE,

    CHECK (length(trim(id)) > 0),
    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(kind)) > 0),
    CHECK (length(trim(library)) > 0),
    CHECK (length(trim(swappability)) > 0),
    CHECK (json_valid(files_json)),
    CHECK (json_type(files_json) = 'array'),
    CHECK (created_at >= 0),
    CHECK (updated_at >= created_at),

    UNIQUE (id, game_id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_components_game_id
    ON components(game_id);

CREATE INDEX IF NOT EXISTS idx_components_game_id_library
    ON components(game_id, library);

CREATE INDEX IF NOT EXISTS idx_components_library
    ON components(library);


-- =============================================================================
-- Library artifacts
-- =============================================================================

CREATE TABLE IF NOT EXISTS library_artifacts (
    id             TEXT    PRIMARY KEY NOT NULL,
    library        TEXT    NOT NULL,
    file_name      TEXT    NOT NULL,
    file_path      TEXT    NOT NULL,
    version        TEXT,
    sha256         TEXT    NOT NULL UNIQUE,
    source         TEXT,
    source_game_id TEXT,
    trust_level    TEXT    NOT NULL,
    created_at     INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    updated_at     INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),

    FOREIGN KEY (source_game_id)
        REFERENCES games(id)
        ON DELETE SET NULL,

    CHECK (length(trim(id)) > 0),
    CHECK (length(trim(library)) > 0),
    CHECK (length(trim(file_name)) > 0),
    CHECK (instr(file_name, '/') = 0),
    CHECK (instr(file_name, '\') = 0),
    CHECK (length(trim(file_path)) > 0),
    CHECK (instr(file_path, char(0)) = 0),
    CHECK (instr(file_path, '\') = 0),
    CHECK (version IS NULL OR length(trim(version)) > 0),
    CHECK (length(sha256) = 64),
    CHECK (lower(sha256) = sha256),
    CHECK (sha256 NOT GLOB '*[^0-9a-f]*'),
    CHECK (source IS NULL OR length(trim(source)) > 0),
    CHECK (source_game_id IS NULL OR length(trim(source_game_id)) > 0),
    CHECK (length(trim(trust_level)) > 0),
    CHECK (created_at >= 0),
    CHECK (updated_at >= created_at),

    UNIQUE (id, library)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_library_artifacts_library
    ON library_artifacts(library);

CREATE INDEX IF NOT EXISTS idx_library_artifacts_source_game_id
    ON library_artifacts(source_game_id);

CREATE INDEX IF NOT EXISTS idx_library_artifacts_updated_at
    ON library_artifacts(updated_at DESC);


-- =============================================================================
-- Operations
-- =============================================================================

CREATE TABLE IF NOT EXISTS operations (
    id            TEXT    PRIMARY KEY NOT NULL,
    game_id       TEXT    NOT NULL,
    kind          TEXT    NOT NULL,
    status        TEXT    NOT NULL,
    created_at    INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    completed_at  INTEGER,
    updated_at    INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    metadata_json TEXT,

    FOREIGN KEY (game_id)
        REFERENCES games(id)
        ON DELETE CASCADE,

    CHECK (length(trim(id)) > 0),
    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(kind)) > 0),
    CHECK (length(trim(status)) > 0),
    CHECK (created_at >= 0),
    CHECK (completed_at IS NULL OR completed_at >= created_at),
    CHECK (updated_at >= created_at),
    CHECK (metadata_json IS NULL OR json_valid(metadata_json)),
    CHECK (metadata_json IS NULL OR json_type(metadata_json) = 'object'),

    UNIQUE (id, game_id)
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
    id            INTEGER PRIMARY KEY,
    operation_id  TEXT    NOT NULL,
    game_id       TEXT    NOT NULL,
    component_id  TEXT    NOT NULL,
    artifact_id   TEXT,
    source_path   TEXT    NOT NULL,
    target_path   TEXT,
    status        TEXT    NOT NULL,
    created_at    INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    updated_at    INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    metadata_json TEXT,

    FOREIGN KEY (operation_id, game_id)
        REFERENCES operations(id, game_id)
        ON DELETE CASCADE,

    FOREIGN KEY (component_id, game_id)
        REFERENCES components(id, game_id)
        ON DELETE CASCADE,

    FOREIGN KEY (artifact_id)
        REFERENCES library_artifacts(id)
        ON DELETE SET NULL,

    CHECK (length(trim(operation_id)) > 0),
    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(component_id)) > 0),
    CHECK (artifact_id IS NULL OR length(trim(artifact_id)) > 0),
    CHECK (length(trim(source_path)) > 0),
    CHECK (instr(source_path, char(0)) = 0),
    CHECK (instr(source_path, '\') = 0),
    CHECK (target_path IS NULL OR length(trim(target_path)) > 0),
    CHECK (target_path IS NULL OR instr(target_path, char(0)) = 0),
    CHECK (target_path IS NULL OR instr(target_path, '\') = 0),
    CHECK (length(trim(status)) > 0),
    CHECK (created_at >= 0),
    CHECK (updated_at >= created_at),
    CHECK (metadata_json IS NULL OR json_valid(metadata_json)),
    CHECK (metadata_json IS NULL OR json_type(metadata_json) = 'object')
) STRICT;

CREATE INDEX IF NOT EXISTS idx_operation_items_operation_id
    ON operation_items(operation_id);

CREATE INDEX IF NOT EXISTS idx_operation_items_game_id
    ON operation_items(game_id);

CREATE INDEX IF NOT EXISTS idx_operation_items_component_id
    ON operation_items(component_id);

CREATE INDEX IF NOT EXISTS idx_operation_items_artifact_id
    ON operation_items(artifact_id);

CREATE INDEX IF NOT EXISTS idx_operation_items_status
    ON operation_items(status);


-- =============================================================================
-- Settings
-- =============================================================================

CREATE TABLE IF NOT EXISTS settings (
    key        TEXT    PRIMARY KEY NOT NULL,
    value      TEXT    NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    updated_at INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),

    CHECK (length(trim(key)) > 0),
    CHECK (length(trim(value)) > 0),
    CHECK (created_at >= 0),
    CHECK (updated_at >= created_at)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_settings_updated_at
    ON settings(updated_at DESC);


-- =============================================================================
-- File hash cache
-- =============================================================================

CREATE TABLE IF NOT EXISTS file_hash_cache (
    path        TEXT    PRIMARY KEY NOT NULL,
    size        INTEGER NOT NULL,
    modified_at INTEGER NOT NULL,
    sha256      TEXT    NOT NULL,
    version     TEXT,
    created_at  INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    updated_at  INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),

    CHECK (length(trim(path)) > 0),
    CHECK (instr(path, char(0)) = 0),
    CHECK (instr(path, '\') = 0),
    CHECK (size >= 0),
    CHECK (modified_at >= 0),
    CHECK (length(sha256) = 64),
    CHECK (lower(sha256) = sha256),
    CHECK (sha256 NOT GLOB '*[^0-9a-f]*'),
    CHECK (version IS NULL OR length(trim(version)) > 0),
    CHECK (created_at >= 0),
    CHECK (updated_at >= created_at)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_file_hash_cache_updated_at
    ON file_hash_cache(updated_at DESC);


-- =============================================================================
-- Cross-table integrity triggers
-- =============================================================================

CREATE TRIGGER IF NOT EXISTS trg_operation_items_artifact_library_insert
BEFORE INSERT ON operation_items
FOR EACH ROW
WHEN NEW.artifact_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'operation_items artifact library mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM components AS c
        JOIN library_artifacts AS a
          ON a.id = NEW.artifact_id
         AND a.library = c.library
        WHERE c.id = NEW.component_id
          AND c.game_id = NEW.game_id
    );
END;

CREATE TRIGGER IF NOT EXISTS trg_operation_items_artifact_library_update
BEFORE UPDATE OF game_id, component_id, artifact_id ON operation_items
FOR EACH ROW
WHEN NEW.artifact_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'operation_items artifact library mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM components AS c
        JOIN library_artifacts AS a
          ON a.id = NEW.artifact_id
         AND a.library = c.library
        WHERE c.id = NEW.component_id
          AND c.game_id = NEW.game_id
    );
END;


-- =============================================================================
-- updated_at touch triggers
-- =============================================================================

CREATE TRIGGER IF NOT EXISTS trg_games_touch_updated_at
AFTER UPDATE ON games
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE games
       SET updated_at = max(
           CAST(unixepoch('subsec') * 1000 AS INTEGER),
           OLD.updated_at + 1
       )
     WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_game_covers_touch_updated_at
AFTER UPDATE ON game_covers
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE game_covers
       SET updated_at = max(
           CAST(unixepoch('subsec') * 1000 AS INTEGER),
           OLD.updated_at + 1
       )
     WHERE game_id = NEW.game_id;
END;

CREATE TRIGGER IF NOT EXISTS trg_components_touch_updated_at
AFTER UPDATE ON components
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE components
       SET updated_at = max(
           CAST(unixepoch('subsec') * 1000 AS INTEGER),
           OLD.updated_at + 1
       )
     WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_library_artifacts_touch_updated_at
AFTER UPDATE ON library_artifacts
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE library_artifacts
       SET updated_at = max(
           CAST(unixepoch('subsec') * 1000 AS INTEGER),
           OLD.updated_at + 1
       )
     WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_operations_touch_updated_at
AFTER UPDATE ON operations
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE operations
       SET updated_at = max(
           CAST(unixepoch('subsec') * 1000 AS INTEGER),
           OLD.updated_at + 1
       )
     WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_operation_items_touch_updated_at
AFTER UPDATE ON operation_items
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE operation_items
       SET updated_at = max(
           CAST(unixepoch('subsec') * 1000 AS INTEGER),
           OLD.updated_at + 1
       )
     WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_settings_touch_updated_at
AFTER UPDATE ON settings
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE settings
       SET updated_at = max(
           CAST(unixepoch('subsec') * 1000 AS INTEGER),
           OLD.updated_at + 1
       )
     WHERE key = NEW.key;
END;

CREATE TRIGGER IF NOT EXISTS trg_file_hash_cache_touch_updated_at
AFTER UPDATE ON file_hash_cache
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE file_hash_cache
       SET updated_at = max(
           CAST(unixepoch('subsec') * 1000 AS INTEGER),
           OLD.updated_at + 1
       )
     WHERE path = NEW.path;
END;