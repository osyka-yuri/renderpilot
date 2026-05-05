PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS games (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    launcher TEXT NOT NULL,
    external_id TEXT,
    platform TEXT NOT NULL,
    runtime TEXT NOT NULL,
    install_path TEXT NOT NULL,
    executable_candidates_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch('subsec') * 1000)
);

CREATE TABLE IF NOT EXISTS components (
    id TEXT PRIMARY KEY NOT NULL,
    game_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    technology TEXT NOT NULL,
    swappability TEXT NOT NULL,
    files_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch('subsec') * 1000),
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_components_game_id ON components(game_id);

CREATE TABLE IF NOT EXISTS library_artifacts (
    id TEXT PRIMARY KEY NOT NULL,
    technology TEXT NOT NULL,
    file_name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    version TEXT,
    sha256 TEXT NOT NULL,
    source TEXT,
    source_game_id TEXT,
    trust_level TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch('subsec') * 1000),
    FOREIGN KEY (source_game_id) REFERENCES games(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS operations (
    id TEXT PRIMARY KEY NOT NULL,
    game_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    completed_at INTEGER,
    metadata_json TEXT,
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_operations_game_id ON operations(game_id);

CREATE TABLE IF NOT EXISTS operation_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id TEXT NOT NULL,
    component_id TEXT NOT NULL,
    artifact_id TEXT,
    source_path TEXT NOT NULL,
    target_path TEXT,
    status TEXT NOT NULL,
    metadata_json TEXT,
    FOREIGN KEY (operation_id) REFERENCES operations(id) ON DELETE CASCADE,
    FOREIGN KEY (component_id) REFERENCES components(id) ON DELETE CASCADE,
    FOREIGN KEY (artifact_id) REFERENCES library_artifacts(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_operation_items_operation_id ON operation_items(operation_id);

CREATE TABLE IF NOT EXISTS backups (
    id TEXT PRIMARY KEY NOT NULL,
    operation_id TEXT NOT NULL,
    game_id TEXT NOT NULL,
    component_id TEXT,
    original_path TEXT NOT NULL,
    backup_path TEXT NOT NULL,
    sha256 TEXT,
    created_at INTEGER NOT NULL,
    metadata_json TEXT,
    FOREIGN KEY (operation_id) REFERENCES operations(id) ON DELETE CASCADE,
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE,
    FOREIGN KEY (component_id) REFERENCES components(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_backups_game_id ON backups(game_id);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch('subsec') * 1000)
);
