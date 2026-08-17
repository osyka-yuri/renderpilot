DROP TRIGGER trg_games_create_scan_authority;
DROP INDEX idx_file_observations_game_path;
DROP INDEX idx_file_observations_artifact_path;
DROP INDEX idx_catalog_scan_authority_readiness;
DROP TABLE file_observations;
DROP TABLE catalog_scan_authority;

CREATE TABLE file_hash_cache (
    path TEXT PRIMARY KEY NOT NULL,
    size INTEGER NOT NULL,
    modified_at INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    version TEXT,
    created_at INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    updated_at INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
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
CREATE INDEX idx_file_hash_cache_updated_at ON file_hash_cache(updated_at DESC);
CREATE TRIGGER trg_file_hash_cache_touch_updated_at
AFTER UPDATE ON file_hash_cache FOR EACH ROW WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE file_hash_cache
       SET updated_at = max(CAST(unixepoch('subsec') * 1000 AS INTEGER), OLD.updated_at + 1)
     WHERE path = NEW.path;
END;

CREATE TABLE scan_source_checkpoints (
    source_key TEXT PRIMARY KEY NOT NULL,
    fingerprint TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    CHECK (length(trim(source_key)) > 0),
    CHECK (length(trim(fingerprint)) > 0),
    CHECK (updated_at >= 0)
) STRICT;
CREATE TRIGGER trg_scan_source_checkpoints_touch_updated_at
AFTER UPDATE ON scan_source_checkpoints
FOR EACH ROW WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE scan_source_checkpoints
       SET updated_at = max(CAST(unixepoch('subsec') * 1000 AS INTEGER), OLD.updated_at + 1)
     WHERE source_key = NEW.source_key;
END;
