-- =============================================================================
-- Migration 0002: NVAPI executable overrides + setting baselines
--
-- Two tables that back the NVAPI/DLSS preset feature:
--
--   nvapi_executable_overrides
--     Per-game pin of which executable RenderPilot should pass to NVAPI
--     when looking up a profile. Overrides the auto-detected top entry
--     from GameInstallation.executable_candidates. Set by the
--     ExecutablePicker UI; cleared when the user picks "Reset to
--     auto-detect".
--
--   nvapi_setting_baselines
--     Snapshot, taken on the *first* RenderPilot write for a given
--     (game, setting), of what the setting looked like before any
--     RenderPilot intervention. Powers "Revert to baseline" and the
--     "modified outside RenderPilot" indicator.
--
-- Notes:
-- - Timestamps are Unix milliseconds (same as 0001_initial.sql).
-- - Both tables CASCADE on games.id delete so a game removal also
--   tears down its NVAPI metadata.
-- - The full live value of a setting is never cached here; the
--   orchestration layer reads it from NVAPI every time. This table
--   only stores the baseline-at-first-write so we can undo our
--   influence later.
-- =============================================================================


CREATE TABLE IF NOT EXISTS nvapi_executable_overrides (
    game_id           TEXT    PRIMARY KEY NOT NULL,
    selected_path     TEXT    NOT NULL,
    selected_basename TEXT    NOT NULL,
    updated_at        INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),

    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(selected_path)) > 0),
    CHECK (instr(selected_path, char(0)) = 0),
    CHECK (length(trim(selected_basename)) > 0),
    CHECK (instr(selected_basename, '/') = 0),
    CHECK (instr(selected_basename, '\') = 0),
    CHECK (updated_at >= 0),

    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE
) STRICT;


CREATE TABLE IF NOT EXISTS nvapi_setting_baselines (
    game_id                 TEXT    NOT NULL,
    setting_key             TEXT    NOT NULL,
    baseline_dword          INTEGER NOT NULL,
    baseline_was_predefined INTEGER NOT NULL,
    predefined_dword        INTEGER,
    captured_exe            TEXT    NOT NULL,
    captured_at             INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),

    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(setting_key)) > 0),
    CHECK (baseline_dword >= 0),
    CHECK (baseline_was_predefined IN (0, 1)),
    CHECK (predefined_dword IS NULL OR predefined_dword >= 0),
    CHECK (length(trim(captured_exe)) > 0),
    CHECK (captured_at >= 0),

    PRIMARY KEY (game_id, setting_key),
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE
) STRICT;
