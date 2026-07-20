/**
 * Wire-level DTOs mirroring the Rust backend (`renderpilot-orchestration::addons::luma`).
 * Field names match the JSON keys produced by serde exactly.
 *
 * Shared shapes come from `@entities/addon`. Only Luma-only shapes live here.
 */

import type {
  CommonAvailabilityOutcome,
  HostActions,
  HostDetection,
  HostFacts,
  MatchConfidence,
  RiskAssessment,
  UpdateStatus,
} from '@entities/addon';

/**
 * Backend-derived actions for Luma's ReShade host (`LumaActions` = shared
 * `HostActions`). Luma never sets `switch_channel` (nightly-only hosts).
 */
export type LumaActions = HostActions;

/** Current install state (`LumaInstallState`, tag `status`). */
export type LumaInstallState =
  | { status: 'not_installed' }
  | {
      status: 'installed';
      /**
       * Installed build label, when known (e.g. `Build 515`, parsed from the
       * upstream rolling release's redirect target). `null` when the build
       * number could not be recovered (e.g. an adopted on-disk install).
       */
      version: string | null;
      /**
       * The add-on's upstream `Last-Modified` HTTP-date string (its publish-date
       * proxy), when the host sent one — the UI's "Add-on dated …" anchor.
       */
      addon_dated: string | null;
      /** When the add-on was first installed (Unix epoch ms). Always present for installed state. */
      installed_at: number;
      /** When the install record was last updated (Unix epoch ms). Always present for installed state. */
      updated_at: number;
      /**
       * Effective ReShade channel of the host artifact, when known.
       * Informational only — Luma has no channel-switch action: every host it
       * writes is nightly; this simply reports whatever a reused foreign host
       * happens to be.
       */
      reshade_channel: string | null;
      /** Launch arguments this title requires (e.g. `-dx11`), re-resolved from
       * the manifest at query time. */
      launch_args: string[];
    };

/**
 * A per-source update report. `null` indicates the source is not tracked or
 * absent.
 */
export type LumaUpdateReport = {
  addon: UpdateStatus | null;
  host: UpdateStatus | null;
  dgvoodoo: UpdateStatus | null;
  overall: UpdateStatus;
};

/** Public identity of a backend-managed dependency. Its source, hashes and
 * installation recipe deliberately never cross the Tauri boundary. */
export type LumaManagedDependencySummary = {
  kind: 'dgvoodoo2';
  version: string;
};

export type LumaFeatureStatus = 'supported' | 'unsupported' | 'experimental' | 'unknown';

export type LumaFeatures = {
  dlss_fsr: LumaFeatureStatus;
  hdr: LumaFeatureStatus;
};

export type LumaGuidanceKind =
  'game_setting' | 'engine_ini' | 'launch_argument' | 'warning' | 'compatibility' | 'external_tool';

/** A reviewed catalogue instruction. `id` is reserved for a local translation
 * override; `fallback_text` keeps the manifest usable before it is translated. */
export type LumaGuidance = {
  id: string;
  kind: LumaGuidanceKind;
  fallback_text: string;
  code?: string;
};

/** Engine family a shared Luma payload targets. */
export type LumaEngine = 'unreal' | 'unity';

/** Dedicated per-game profile vs shared engine payload. */
export type LumaProfile = { scope: 'game' } | { scope: 'engine'; engine: LumaEngine };

/** Installability verdict (`AvailabilityOutcome`, tag `kind`). */
export type AvailabilityOutcome =
  | (Extract<CommonAvailabilityOutcome, { kind: 'installable' }> & {
      kind: 'installable';
      /** Honest confidence shown as a badge. */
      confidence: MatchConfidence;
      risk: RiskAssessment;
      /** Required launch arguments, shown as a copyable callout. */
      launch_args: string[];
      /** Dedicated game profile vs shared engine payload (UI badge source). */
      profile: LumaProfile;
      /** Present only for Generic UE profiles with an explicit Wiki matrix row. */
      features: LumaFeatures | null;
      /** Reviewed instructions; raw Wiki notes are never returned here. */
      guidance: LumaGuidance[];
      /** Runtime dependency required by this profile; an existing compatible
       * runtime is reused rather than installed again. */
      external_requirement: LumaManagedDependencySummary | null;
    })
  | Exclude<CommonAvailabilityOutcome, { kind: 'installable' }>;

/** Read-only preview returned by `luma_availability`. */
export type AvailabilityReport = {
  state: LumaInstallState;
  host_detection: HostDetection;
  host_facts: HostFacts;
  actions: LumaActions;
  /** Minimum ReShade host version Luma's current builds require. */
  min_reshade_version: string;
  /** Advisory Visual C++ Redistributable presence check. `null` when it could
   * not be determined. Never blocks an install — informational only. */
  vcredist_present: boolean | null;
  /** Official installer URL for the redistributable this game's detected
   * architecture needs (x64 or x86) — the advisory callout's download link. */
  vcredist_installer_url: string;
  /** Whether a prior install or rollback for this game did not complete
   * cleanly. Reinstalling clears it. */
  install_torn: boolean;
  outcome: AvailabilityOutcome;
};
