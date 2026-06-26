/**
 * Wire-level DTOs mirroring the Rust backend (`renderpilot-orchestration::renodx`).
 * Field names match the JSON keys produced by serde exactly.
 */

/** Graphics API as serialized by `GraphicsApi` (`#[serde(rename = "D3D11")]`, …). */
export type GraphicsApi = 'D3D9' | 'D3D10' | 'D3D11' | 'D3D12' | 'OpenGl' | 'Vulkan' | 'Unknown';

/**
 * Confidence that an install will work (`MatchConfidence`), from the wiki
 * test-map status and how the match was made: `verified` (listed working),
 * `experimental` (listed WIP), `untested` (listed-but-untested or matched only
 * by engine — a generic guess).
 */
export type MatchConfidence = 'verified' | 'experimental' | 'untested';

/** Effective install risk severity. */
export type RiskSeverity = 'info' | 'warn' | 'block';

/** Anti-cheat engine classification. */
export type AnticheatEngine = 'eac' | 'battleye' | 'none' | 'unknown';

/** Online/multiplayer classification. */
export type OnlineKind = 'singleplayer' | 'coop' | 'pvp' | 'unknown';

/** Assessment confidence. */
export type AssessmentConfidence = 'high' | 'medium' | 'low';

/** Ban/stability risk assessment for installing RenoDX. */
export type RiskAssessment = {
  severity: RiskSeverity;
  anticheat_engine: AnticheatEngine;
  online: OnlineKind;
  message_key: string;
  confidence: AssessmentConfidence;
  source: string | null;
  detected_locally: boolean;
};

/** Why a matched title cannot be installed (`IncompatibilityReason`, tag `reason`). */
export type IncompatibilityReason =
  | { reason: 'api_unsupported'; detected: GraphicsApi }
  | { reason: 'api_not_allowed'; detected: GraphicsApi; required: GraphicsApi[] }
  | { reason: 'arch_unknown' };

/** Current install state (`RenoDxInstallState`, tag `status`). */
export type RenoDxInstallState =
  | { status: 'not_installed' }
  | {
      status: 'installed';
      version: string | null;
      reshade_managed_by_us: boolean;
      /**
       * The add-on's upstream `Last-Modified` HTTP-date string (its publish-date
       * proxy), when the host sent one. Parsed/formatted on the UI as the
       * "Add-on dated …" anchor. RenoDX add-ons are rolling snapshots with no
       * version number, so this is the concrete freshness anchor.
       */
      addon_dated: string | null;
      /** When the add-on was first installed (Unix epoch ms), when known. */
      installed_at: number | null;
      /** When the install record was last updated (Unix epoch ms), when known. */
      updated_at: number | null;
      /**
       * Whether the install includes the DLSS-Fix companion add-on. Surfaced
       * directly on the state (rather than derived from the update report) so it
       * stays correct while the update probe is in flight or after it failed.
       */
      dlss_fix_installed: boolean;
      /**
       * Whether the add-on has a tracked upstream source (a normal install).
       * `false` for a user-file install. Surfaced directly (not inferred from the
       * update report's `addon`, which is also `null` mid-probe / on failure) so
       * the "installed from a file" hint stays correct.
       */
      addon_tracked: boolean;
    };

/**
 * Whether an installed add-on has an upstream update (`UpdateStatus`). RenoDX
 * ships rolling snapshots, so `unknown` (network failure / no recorded source)
 * is a normal, non-error result.
 */
export type UpdateStatus = 'current' | 'available' | 'unknown';

/**
 * Single freshness verdict the card renders as a status pill. Derived in the
 * store from the update report and probe state:
 * - `checking`  — a probe is in flight (suppresses a transient verdict).
 * - `available` — some tracked source has an upstream update.
 * - `untracked` — nothing is tracked (a file install with a foreign/absent host).
 * - `current`   — every tracked source is up to date.
 * - `unknown`   — no verdict yet, or the check failed (network).
 */
export type RenoDxFreshness = 'checking' | 'available' | 'untracked' | 'current' | 'unknown';

/**
 * A per-source update report. null indicates the source is not tracked
 * (e.g. file install) or absent (e.g. foreign host).
 */
export type RenoDxUpdateReport = {
  addon: UpdateStatus | null;
  host: UpdateStatus | null;
  dlssFix: UpdateStatus | null;
  overall: UpdateStatus;
};
/** Installability verdict (`AvailabilityOutcome`, tag `kind`). */
export type AvailabilityOutcome =
  | {
      kind: 'installable';
      /** Honest confidence shown as a badge. */
      confidence: MatchConfidence;
      risk: RiskAssessment;
      /** i18n note/requirement keys (a generic install carries its engine label here). */
      notes_keys: string[];
    }
  /**
   * The add-on is distributed off-GitHub (Discord/Nexus): link the user out, and
   * — when `file_install` is present (the game is compatible) — also let them
   * install a file they downloaded themselves.
   */
  | {
      kind: 'external';
      url: string;
      label_key: string;
      file_install: {
        confidence: MatchConfidence;
        risk: RiskAssessment;
        notes_keys: string[];
      } | null;
    }
  /** The game already has native HDR; RenoDX is not offered. */
  | { kind: 'native_hdr' }
  | { kind: 'incompatible'; reason: IncompatibilityReason }
  /** Blacklisted / known-broken, with an optional i18n reason key. */
  | { kind: 'blacklisted'; reason: string | null }
  /** No RenoDX profile matched the game. */
  | { kind: 'unsupported' };

/**
 * The manual "install ReShade host + your own add-on file" escape hatch, present
 * for a DirectX game with no automatic or curated-external path.
 */
export type ManualFileInstall = {
  risk: RiskAssessment;
  /** Catalogue add-on stem (`renodx-<slug>`) for a soft filename check, or null. */
  expected_addon_name: string | null;
  /** The game's architecture (`x64` / `x86`) for an add-on-arch check, or null. */
  game_arch: 'x64' | 'x86' | null;
};

/** Read-only preview returned by `renodx_availability`. */
export type AvailabilityReport = {
  state: RenoDxInstallState;
  outcome: AvailabilityOutcome;
  manual_install: ManualFileInstall | null;
};
