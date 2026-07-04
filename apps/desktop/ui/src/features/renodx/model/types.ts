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

/**
 * How RenoDX hooks into a game (`HostKind`): a per-game ReShade proxy DLL
 * (Direct3D) or the shared ReShade Vulkan layer (Vulkan games).
 */
export type HostKind = 'proxy' | 'vulkan';

export type ActionConfirmationScope = 'anticheat' | 'all_vulkan_reno_dx_games';

export type ActionDisabledReason =
  | 'blocked_by_conflict'
  | 'blocked_by_risk'
  | 'stable_unavailable'
  | 'read_only'
  | 'unsupported'
  | 'validation_required';

/** Backend-authored action rights and disablement (`ActionDescriptor`). */
export type ActionDescriptor = {
  enabled: boolean;
  requires_confirmation: boolean;
  confirmation_scope: ActionConfirmationScope | null;
  disabled_reason: ActionDisabledReason | null;
  target_channel: ReshadeChannel | null;
};

/** Per-slot ReShade host actions the backend currently permits (`RenoDxActions`). */
export type RenoDxActions = {
  install?: ActionDescriptor;
  use_existing?: ActionDescriptor;
  repair?: ActionDescriptor;
  update?: ActionDescriptor;
  switch_channel?: ActionDescriptor;
  resolve_conflict?: ActionDescriptor;
};

/** Whether a ReShade proxy host is present in the game's slot (`HostDetection`). */
export type HostDetection = 'absent' | 'present' | 'conflict';
export type HostAddonSupport = 'full' | 'limited' | 'unknown';
/** Freshness of the detected proxy host (`HostUpdateStatus`). */
export type HostUpdateStatus =
  | 'current'
  | 'update_available'
  | 'repair_available'
  | 'unknown_needs_validation'
  | 'channel_mismatch';

export type HostChannelFacts = {
  selected: ReshadeChannel;
  effective: ReshadeChannel;
  detected: ReshadeChannel | null;
};

/** Observable state of the detected proxy host (`HostFacts`). */
export type HostFacts = {
  slot: string | null;
  active: boolean;
  path: string | null;
  version: string | null;
  addon_support: HostAddonSupport;
  channel: HostChannelFacts;
  update_status: HostUpdateStatus;
  /** Whether the active slot is a recognized non-ReShade build (e.g. GShade)
   * RenoDX never checks for updates or replaces automatically. */
  is_custom_build: boolean;
};

/** Shared Vulkan layer detection state (`VulkanLayerDetection`). Never encodes
 * install origin; action rights come only from `VulkanLayerActions`. */
export type VulkanLayerDetection =
  | 'not_installed'
  | 'installed'
  | 'installed_disabled'
  | 'external_read_only'
  | 'conflict'
  | 'unsupported';

export type VulkanLoaderVisibility = 'normal' | 'hkcu_not_visible_when_elevated' | 'ambiguous';

export type VulkanLayerArchitecture = 'x64' | 'x86' | 'unknown';

/** Closed diagnostics for a read-only/conflict/broken layer state, ordered by
 * display priority (`LayerDiagnosticReason`). */
export type LayerDiagnosticReason =
  | 'external_layer_detected'
  | 'duplicate_layer_manifest'
  | 'ambiguous_loader_visibility'
  | 'missing_layer_dll'
  | 'unreadable_dll'
  | 'missing_manifest'
  | 'registry_missing'
  | 'registry_disabled'
  | 'unsupported_architecture'
  | 'hkcu_not_visible_when_elevated'
  | 'manifest_malformed'
  | 'registry_scope_not_writable'
  | 'permission_denied'
  | 'backend_validation_failed'
  | 'hash_mismatch'
  | 'db_only_fallback';

/** Observable state of the shared Vulkan layer (`VulkanLayerFacts`). */
export type VulkanLayerFacts = {
  manifest_path: string | null;
  dll_path: string | null;
  version: string | null;
  architecture: VulkanLayerArchitecture;
  loader_visibility: VulkanLoaderVisibility;
};

/** Backend-authored actions for the shared Vulkan layer (`VulkanLayerActions`). */
export type VulkanLayerActions = {
  install?: ActionDescriptor;
  update?: ActionDescriptor;
  switch_channel?: ActionDescriptor;
  remove?: ActionDescriptor;
  resolve_conflict?: ActionDescriptor;
};

/** Full shared Vulkan layer report returned by the availability preview
 * (`VulkanLayerReport`). */
export type VulkanLayerReport = {
  layer_detection: VulkanLayerDetection;
  layer_facts: VulkanLayerFacts;
  diagnostic_reasons: LayerDiagnosticReason[];
  actions: VulkanLayerActions;
};

export type VulkanLayerManagementReport = {
  layer: VulkanLayerReport;
  reshade_stable_supported: boolean;
  recorded_channel: ReshadeChannel | null;
  default_channel: ReshadeChannel;
  update_status?: UpdateStatus;
};

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
  reference_url: string | null;
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
      /** Host mechanism used by this install; null for legacy records. */
      host_kind: HostKind | null;
      version: string | null;
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
export type UpdateStatus =
  'current' | 'available' | 'unknown' | 'channel_mismatch' | 'unknown_needs_validation';

export type ReshadeChannel = 'stable' | 'nightly';

export type RenoDxAddonLoadMode = 'auto_search' | 'load_from_dll_main' | 'unknown';

export type RenoDxAddonState = {
  present_on_disk: boolean;
  expected_path: string;
  discovered_path: string | null;
  enabled_by_config: boolean | null;
  load_mode: RenoDxAddonLoadMode;
};

/**
 * Single freshness verdict the card renders as a status pill. Derived in the
 * store from the update report and probe state:
 * - `checking`  — a probe is in flight (suppresses a transient verdict).
 * - `available` — some tracked source has an upstream update.
 * - `untracked` — no upstream add-on source is tracked.
 * - `current`   — every tracked source is up to date.
 * - `unknown`   — no verdict yet, or the check failed (network).
 */
export type RenoDxFreshness = 'checking' | 'available' | 'untracked' | 'current' | 'unknown';

/**
 * A per-source update report. null indicates the source is not tracked
 * (e.g. file install) or absent.
 */
export type RenoDxUpdateReport = {
  addon: UpdateStatus | null;
  host: UpdateStatus | null;
  dlssFix: UpdateStatus | null;
  overall: UpdateStatus;
  /** Vulkan-layer digest-mismatch diagnostics (empty for proxy installs). */
  vulkan_diagnostics?: LayerDiagnosticReason[];
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
      /** Proxy DLL or the shared Vulkan layer. */
      host_kind: HostKind;
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
        /** Proxy DLL or the shared Vulkan layer. */
        host_kind: HostKind;
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
 * when no automatic or curated-external path is available.
 */
export type ManualFileInstall = {
  risk: RiskAssessment;
  /** Proxy DLL or the shared Vulkan layer. */
  host_kind: HostKind;
  /** Catalogue add-on stem (`renodx-<slug>`) for a soft filename check, or null. */
  expected_addon_name: string | null;
  /** The game's architecture (`x64` / `x86`) for an add-on-arch check, or null. */
  game_arch: 'x64' | 'x86' | null;
};

/** Read-only preview returned by `renodx_availability`. */
export type AvailabilityReport = {
  state: RenoDxInstallState;
  host_detection: HostDetection;
  host_facts: HostFacts;
  actions: RenoDxActions;
  reshade_stable_supported: boolean;
  renodx_addon: RenoDxAddonState | null;
  outcome: AvailabilityOutcome;
  manual_install: ManualFileInstall | null;
  vulkan_layer: VulkanLayerReport;
};
