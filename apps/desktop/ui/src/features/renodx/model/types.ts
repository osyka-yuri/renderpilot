/**
 * Wire-level DTOs mirroring the Rust backend (`renderpilot-orchestration::renodx`).
 * Field names match the JSON keys produced by serde exactly.
 *
 * Shared shapes come from `@entities/addon`. Only RenoDX-only shapes live here.
 */

import type {
  ActionDescriptor,
  CatalogMessage,
  CommonAvailabilityOutcome,
  HostActions,
  HostDetection,
  HostFacts,
  MatchConfidence,
  ReshadeChannel,
  UpdateStatus,
  EngineConfigAvailability,
} from '@entities/addon';

/**
 * How RenoDX hooks into a game (`HostKind`): a per-game ReShade proxy DLL
 * (Direct3D) or the shared ReShade Vulkan layer (Vulkan games).
 */
export type HostKind = 'proxy' | 'vulkan';

/** Per-slot ReShade host actions (`RenoDxActions` = shared `HostActions`). */
export type RenoDxActions = HostActions;

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
      /** When the add-on was first installed (Unix epoch ms). Always present for installed state. */
      installed_at: number;
      /** When the install record was last updated (Unix epoch ms). Always present for installed state. */
      updated_at: number;
      /**
       * Whether the install record contains any DLSS-Fix ownership or source
       * evidence. Exact file presence and allowed actions come from the dedicated
       * DLSS-Fix availability projection.
       */
      dlss_fix_evidence_present: boolean;
      /**
       * Whether the add-on has a tracked upstream source (a normal install).
       * `false` for a user-file install. Surfaced directly (not inferred from the
       * update report's `addon`, which is also `null` mid-probe / on failure) so
       * the "installed from a file" hint stays correct.
       */
      addon_tracked: boolean;
    };

export type RenoDxAddonLoadMode = 'auto_search' | 'load_from_dll_main' | 'unknown';

export type RenoDxAddonState = {
  present_on_disk: boolean;
  expected_path: string;
  discovered_path: string | null;
  enabled_by_config: boolean | null;
  load_mode: RenoDxAddonLoadMode;
};

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

/** Exact DLSS-Fix ownership relationship returned by the backend. */
export type DlssFixBindingState = 'none' | 'source_only' | 'owned_only' | 'bound' | 'invalid';

/** A dedicated DLSS-Fix action, independent of generic RenoDX update policy. */
export type DlssFixAction =
  | 'retry_recovery'
  | 'install'
  | 'update'
  | 'repair'
  | 'remove'
  | 'validation_required';

/** Backend-authored DLSS-Fix action projection. */
export type DlssFixAvailability =
  | {
      /** A durable DLSS-Fix mutation needs recovery before normal RenoDX state is available. */
      kind: 'recovery_pending';
      actions: ['retry_recovery'];
    }
  | {
      /** Normal per-install companion ownership/action projection. */
      kind: 'binding';
      state: DlssFixBindingState;
      actions: DlssFixAction[];
    };

/** Detected / generic engine identity used for RenoDX engine-profile installs. */
export type RenoDxEngine = 'unreal' | 'unreal_extended' | 'unity';

/** User-facing identity of an engine-level generic catalogue match. */
export type RenoDxGenericProfile = {
  engine: RenoDxEngine;
  message: CatalogMessage;
  profile_id: string | null;
};

export type RenoDxGuidanceKind =
  | 'game_setting'
  | 'addon_setting'
  | 'engine_ini'
  | 'warning'
  | 'compatibility'
  | 'external_tool';

export type RenoDxSetting = {
  name: string;
  value: string;
};

/** Reviewed RenoDX recommendation. Engine/version conditions are resolved by the backend. */
export type RenoDxGuidance = {
  id: string;
  kind: RenoDxGuidanceKind;
  message_id: string;
  fallback_text: string;
  code: string | null;
  settings: RenoDxSetting[];
  url: string | null;
};

export type RenoDxLaunchRequirement = {
  arguments: string[];
  requirement: 'required' | 'recommended';
};

/** Installability verdict (`AvailabilityOutcome`, tag `kind`). */
export type AvailabilityOutcome =
  | (Extract<CommonAvailabilityOutcome, { kind: 'installable' }> & {
      kind: 'installable';
      /** Honest confidence shown as a badge. */
      confidence: MatchConfidence;
      /** Present when this install comes from an engine-level generic profile. */
      generic_profile: RenoDxGenericProfile | null;
      /** Stable manifest profile selected for this title/profile. */
      profile_id: string | null;
      /** Proxy DLL or the shared Vulkan layer. */
      host_kind: HostKind;
      guidance: RenoDxGuidance[];
      launch: RenoDxLaunchRequirement | null;
    })
  /**
   * The add-on is distributed off-GitHub (Discord/Nexus): link the user out, and
   * — when `file_install` is present (the game is compatible) — also let them
   * install a file they downloaded themselves.
   */
  | {
      kind: 'external';
      url: string;
      message: CatalogMessage;
      file_install: {
        confidence: MatchConfidence;
        /** Proxy DLL or the shared Vulkan layer. */
        host_kind: HostKind;
        generic_profile: RenoDxGenericProfile | null;
        profile_id: string | null;
        guidance: RenoDxGuidance[];
        launch: RenoDxLaunchRequirement | null;
      } | null;
    }
  /** The game already has native HDR; RenoDX is not offered. */
  | { kind: 'native_hdr' }
  /** This title's configuration contains settings this version of RenderPilot does not support. */
  | { kind: 'unsupported_settings' }
  | Exclude<CommonAvailabilityOutcome, { kind: 'installable' | 'unmanaged_present' }>;

/**
 * The manual "install ReShade host + your own add-on file" escape hatch, present
 * when no automatic or curated-external path is available.
 */
export type ManualFileInstall = {
  /** Proxy DLL or the shared Vulkan layer. */
  host_kind: HostKind;
  /** Catalogue add-on stem (`renodx-<slug>`) for a soft filename check, or null. */
  expected_addon_name: string | null;
  /** The game's architecture (`x64` / `x86`) for an add-on-arch check, or null. */
  game_arch: 'x64' | 'x86' | null;
};

/** Read-only preview returned by `renodx_availability`. */
export type AvailabilityReport = {
  engine_config?: EngineConfigAvailability;
  state: RenoDxInstallState;
  host_detection: HostDetection;
  host_facts: HostFacts;
  actions: RenoDxActions;
  reshade_stable_supported: boolean;
  renodx_addon: RenoDxAddonState | null;
  install_torn: boolean;
  outcome: AvailabilityOutcome;
  manual_install: ManualFileInstall | null;
  vulkan_layer: VulkanLayerReport;
};
