//! DDL for catalog scan authority and owner-scoped strong observations.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

/// Tables and indexes introduced with schema v17.
pub(super) const SQL: &str = r#"
CREATE TABLE IF NOT EXISTS catalog_scan_authority (
    game_id             TEXT    PRIMARY KEY NOT NULL,
    readiness           TEXT    NOT NULL,
    authority_epoch     INTEGER NOT NULL DEFAULT 0,
    invalidation_reason TEXT,
    mutation_token      TEXT,
    completed_at        INTEGER,
    updated_at          INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE,
    CHECK (readiness IN ('never_completed', 'complete', 'invalidated')),
    CHECK (authority_epoch >= 0),
    CHECK ((readiness = 'complete' AND invalidation_reason IS NULL AND mutation_token IS NULL AND completed_at IS NOT NULL)
        OR (readiness = 'never_completed' AND invalidation_reason IS NULL AND mutation_token IS NULL AND completed_at IS NULL)
        OR (readiness = 'invalidated' AND length(trim(invalidation_reason)) > 0 AND completed_at IS NULL)),
    CHECK (mutation_token IS NULL OR length(trim(mutation_token)) > 0),
    CHECK (updated_at >= 0)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_catalog_scan_authority_readiness
    ON catalog_scan_authority(readiness, updated_at DESC);

CREATE TABLE IF NOT EXISTS file_observations (
    owner_kind          TEXT    NOT NULL,
    owner_id            TEXT    NOT NULL,
    game_id             TEXT,
    artifact_id         TEXT,
    normalized_path     TEXT    NOT NULL,
    identity_kind       TEXT    NOT NULL,
    object_identity     TEXT    NOT NULL,
    change_token        TEXT    NOT NULL,
    size                INTEGER NOT NULL,
    algorithm_revision  INTEGER NOT NULL,
    sha256              TEXT    NOT NULL,
    version_observed    INTEGER NOT NULL,
    version             TEXT,
    runtime_observed    INTEGER NOT NULL,
    runtime_json        TEXT,
    pe_observed         INTEGER NOT NULL,
    pe_json             TEXT,
    created_at          INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    updated_at          INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE,
    FOREIGN KEY (artifact_id) REFERENCES library_artifacts(id) ON DELETE CASCADE,
    CHECK (owner_kind IN ('game', 'artifact')),
    CHECK ((owner_kind = 'game' AND owner_id = game_id AND game_id IS NOT NULL AND artifact_id IS NULL)
        OR (owner_kind = 'artifact' AND owner_id = artifact_id AND game_id IS NULL AND artifact_id IS NOT NULL)),
    CHECK (length(trim(owner_id)) > 0),
    CHECK (length(trim(normalized_path)) > 0 AND instr(normalized_path, char(0)) = 0 AND instr(normalized_path, '\\') = 0),
    CHECK (length(trim(identity_kind)) > 0 AND length(trim(object_identity)) > 0 AND length(trim(change_token)) > 0),
    CHECK (size >= 0 AND algorithm_revision >= 0),
    CHECK (length(sha256) = 64 AND lower(sha256) = sha256 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    CHECK (version_observed IN (0, 1) AND (version_observed = 1 OR version IS NULL)),
    CHECK (version IS NULL OR length(trim(version)) > 0),
    CHECK (runtime_observed IN (0, 1) AND (runtime_observed = 1 OR runtime_json IS NULL)),
    CHECK (runtime_json IS NULL OR (json_valid(runtime_json) AND json_type(runtime_json) = 'object')),
    CHECK (pe_observed IN (0, 1) AND (pe_observed = 1 OR pe_json IS NULL)),
    CHECK (pe_json IS NULL OR (json_valid(pe_json) AND json_type(pe_json) = 'object')),
    CHECK (created_at >= 0 AND updated_at >= created_at),
    UNIQUE (owner_kind, owner_id, normalized_path)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_file_observations_game_path
    ON file_observations(game_id, normalized_path) WHERE owner_kind = 'game';
CREATE INDEX IF NOT EXISTS idx_file_observations_artifact_path
    ON file_observations(artifact_id, normalized_path) WHERE owner_kind = 'artifact';

CREATE TRIGGER IF NOT EXISTS trg_games_create_scan_authority
AFTER INSERT ON games FOR EACH ROW
BEGIN
    INSERT INTO catalog_scan_authority (game_id, readiness, authority_epoch, updated_at)
    VALUES (NEW.id, 'never_completed', 0, NEW.created_at)
    ON CONFLICT(game_id) DO NOTHING;
END;
"#;

pub(crate) fn apply(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(SQL)
        .map_err(|error| storage_context("could not create v17 observation schema", error))
}
