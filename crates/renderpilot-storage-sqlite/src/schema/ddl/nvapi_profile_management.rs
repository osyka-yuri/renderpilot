//! Durable ownership, receipts, shared setting claims, and driver mutation journal.

pub(crate) const fn baseline_sql() -> &'static str {
    r#"
CREATE TABLE IF NOT EXISTS nvapi_owned_profiles (
    game_id                    TEXT PRIMARY KEY NOT NULL,
    profile_name               TEXT NOT NULL UNIQUE,
    binding_path               TEXT NOT NULL,
    application_witness_json   TEXT NOT NULL,
    composition_json            TEXT NOT NULL,
    state                       TEXT NOT NULL DEFAULT 'active',
    updated_at                  INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(profile_name)) > 0),
    CHECK (length(trim(binding_path)) > 0),
    CHECK (json_valid(application_witness_json) AND json_type(application_witness_json) = 'object'),
    CHECK (json_valid(composition_json) AND json_type(composition_json) = 'object'),
    CHECK (state IN ('active', 'pending', 'conflict')),
    CHECK (updated_at >= 0),
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE IF NOT EXISTS nvapi_drs_targets (
    target_id         TEXT PRIMARY KEY NOT NULL,
    profile_name      TEXT NOT NULL UNIQUE,
    profile_is_predefined INTEGER NOT NULL,
    kind              TEXT NOT NULL,
    identity_json     TEXT NOT NULL,
    CHECK (length(trim(target_id)) > 0),
    CHECK (length(trim(profile_name)) > 0),
    CHECK (profile_is_predefined IN (0, 1)),
    CHECK (kind IN ('external', 'renderpilot')),
    CHECK (json_valid(identity_json) AND json_type(identity_json) = 'object')
) STRICT;

CREATE TABLE IF NOT EXISTS nvapi_drs_target_app_witnesses (
    target_id             TEXT NOT NULL,
    executable_path       TEXT NOT NULL,
    application_json      TEXT NOT NULL,
    updated_at            INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    CHECK (length(trim(executable_path)) > 0),
    CHECK (json_valid(application_json) AND json_type(application_json) = 'object'),
    CHECK (updated_at >= 0),
    PRIMARY KEY (target_id, executable_path),
    FOREIGN KEY (target_id) REFERENCES nvapi_drs_targets(target_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE IF NOT EXISTS nvapi_target_setting_claims (
    target_id              TEXT NOT NULL,
    setting_id             INTEGER NOT NULL,
    original_present       INTEGER NOT NULL,
    original_value         INTEGER,
    expected_present       INTEGER NOT NULL,
    expected_value         INTEGER,
    updated_at             INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    CHECK (setting_id BETWEEN 0 AND 4294967295),
    CHECK (original_present IN (0, 1)),
    CHECK (expected_present IN (0, 1)),
    CHECK ((original_present = 0 AND original_value IS NULL) OR (original_present = 1 AND original_value IS NOT NULL AND original_value BETWEEN 0 AND 4294967295)),
    CHECK ((expected_present = 0 AND expected_value IS NULL) OR (expected_present = 1 AND expected_value IS NOT NULL AND expected_value BETWEEN 0 AND 4294967295)),
    CHECK (updated_at >= 0),
    PRIMARY KEY (target_id, setting_id),
    FOREIGN KEY (target_id) REFERENCES nvapi_drs_targets(target_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE IF NOT EXISTS nvapi_game_claim_refs (
    game_id       TEXT NOT NULL,
    target_id     TEXT NOT NULL,
    setting_id    INTEGER NOT NULL,
    executable_path TEXT NOT NULL,
    CHECK (setting_id BETWEEN 0 AND 4294967295),
    CHECK (length(trim(executable_path)) > 0),
    PRIMARY KEY (game_id, target_id, setting_id),
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE RESTRICT,
    FOREIGN KEY (target_id, setting_id)
        REFERENCES nvapi_target_setting_claims(target_id, setting_id) ON DELETE RESTRICT
) STRICT;

CREATE TABLE IF NOT EXISTS pending_drs_operations (
    op_id           TEXT PRIMARY KEY NOT NULL,
    game_id         TEXT,
    target_id       TEXT,
    kind            TEXT NOT NULL,
    before_json     TEXT NOT NULL,
    after_json      TEXT NOT NULL,
    observed_json   TEXT,
    phase           TEXT NOT NULL DEFAULT 'intent',
    created_at      INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    updated_at      INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    CHECK (length(trim(op_id)) > 0),
    CHECK (game_id IS NULL OR length(trim(game_id)) > 0),
    CHECK (target_id IS NULL OR length(trim(target_id)) > 0),
    CHECK (kind IN ('create_profile', 'delete_profile', 'move_profile', 'setting')),
    CHECK (json_valid(before_json)),
    CHECK (json_valid(after_json)),
    CHECK (observed_json IS NULL OR json_valid(observed_json)),
    CHECK (phase IN ('intent', 'driver_saved', 'conflict')),
    CHECK (created_at >= 0 AND updated_at >= created_at),
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX IF NOT EXISTS idx_nvapi_game_claim_refs_target
    ON nvapi_game_claim_refs(target_id, setting_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_nvapi_owned_profiles_binding_path
    ON nvapi_owned_profiles(lower(binding_path));
CREATE INDEX IF NOT EXISTS idx_pending_drs_operations_phase_target
    ON pending_drs_operations(phase, target_id, game_id);

CREATE TRIGGER IF NOT EXISTS trg_games_restrict_nvapi_owned_delete
BEFORE DELETE ON games
WHEN EXISTS (SELECT 1 FROM nvapi_owned_profiles WHERE game_id = OLD.id)
  OR EXISTS (SELECT 1 FROM nvapi_game_claim_refs WHERE game_id = OLD.id)
  OR EXISTS (SELECT 1 FROM pending_drs_operations WHERE game_id = OLD.id)
BEGIN
    SELECT RAISE(ABORT, 'NVAPI state must be released or resolved before deleting a game');
END;
"#
}
