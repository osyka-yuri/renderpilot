/**
 * Wire-level DTOs genuinely shared by every add-on tool (RenoDX, Luma, …) —
 * backed by the same Rust types (`addons::matching`, `addons::reshade::dto`,
 * `addons::update`). Field names match the JSON keys
 * produced by serde exactly. Tool-specific shapes (actions, install state,
 * update reports, availability outcomes) stay in each feature's own
 * `model/types.ts`, since they genuinely diverge per tool.
 */

/** Graphics API as serialized by `GraphicsApi` (`#[serde(rename = "D3D11")]`, …). */
export type GraphicsApi = 'D3D9' | 'D3D10' | 'D3D11' | 'D3D12' | 'OpenGl' | 'Vulkan' | 'Unknown';

/** Executable architecture (`Architecture`). */
export type Architecture = 'X86' | 'X64';

/**
 * Confidence that an install will work (`MatchConfidence`), from the wiki
 * test-map status and how the match was made: `verified` (listed working),
 * `experimental` (listed WIP), `untested` (listed-but-untested or matched only
 * by engine — a generic guess).
 */
export type MatchConfidence = 'verified' | 'experimental' | 'untested';

/** ReShade host download/build channel (`ReshadeChannel`). */
export type ReshadeChannel = 'stable' | 'nightly';

/** Fresh backend-issued context tokens for risk-increasing mutations. */
export type MutationSafetyScope = 'game' | 'game_and_shared';

export type MutationSafetyTokens = {
  gameContextToken: string;
  sharedVulkanContextToken?: string | null;
};

/** Narrows persisted or otherwise untrusted channel metadata to the public enum. */
export function isReshadeChannel(value: unknown): value is ReshadeChannel {
  return value === 'stable' || value === 'nightly';
}

/** Stable localization id plus the catalogue's reviewed English fallback. */
export type CatalogMessage = {
  id: string;
  fallback_text: string;
};

// Import via the shared *segment* public API (not direct file, not root).
// Re-assign to avoid "re-export from alias" lint rule while keeping the
// @entities/addon barrel working for existing callers.
import type { AddonKind as _AddonKind } from '@shared/model';

export type AddonKind = _AddonKind;

export type ActionConfirmationScope = 'all_vulkan_reno_dx_games';

export type ActionDisabledReason =
  | 'blocked_by_conflict'
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

/** Whether a ReShade proxy host is present in the game's slot (`HostDetection`). */
export type HostDetection = 'absent' | 'present' | 'conflict';
export type HostAddonSupport = 'full' | 'limited' | 'unknown';
/** Freshness of the detected host (`HostUpdateStatus`). */
export type HostUpdateStatus =
  | 'current'
  | 'update_available'
  | 'repair_available'
  | 'unknown_needs_validation'
  | 'channel_mismatch';

export type HostChannelFacts = {
  selected: ReshadeChannel;
  detected: ReshadeChannel | null;
};

/** Observable state of the detected ReShade host (`HostFacts`). */
export type HostFacts = {
  slot: string | null;
  active: boolean;
  path: string | null;
  version: string | null;
  addon_support: HostAddonSupport;
  channel: HostChannelFacts;
  update_status: HostUpdateStatus;
  /** Whether the active slot is a recognized non-ReShade build (e.g. GShade) —
   * a tool never checks for updates or replaces it automatically. */
  is_custom_build: boolean;
};

/**
 * Backend-derived host actions shared by every ReShade-hosted tool (`HostActions`).
 * `switch_channel` is only set by tools that support channel switching (RenoDX).
 */
export type HostActions = {
  install?: ActionDescriptor;
  use_existing?: ActionDescriptor;
  repair?: ActionDescriptor;
  update?: ActionDescriptor;
  switch_channel?: ActionDescriptor;
  resolve_conflict?: ActionDescriptor;
};

/** Effective install risk severity. */
/**
 * Why a matched title cannot be installed (`IncompatibilityReason`, tag
 * `reason`). Not every tool's backend emits every variant (RenoDX targets any
 * DirectX version and never hard-gates on architecture, so it never emits
 * `arch_mismatch`), but the wire type is the same Rust enum for all of them.
 */
export type IncompatibilityReason =
  | { reason: 'api_unsupported'; detected: GraphicsApi }
  | { reason: 'api_not_allowed'; detected: GraphicsApi; required: GraphicsApi[] }
  | { reason: 'arch_unknown' }
  | { reason: 'arch_mismatch'; detected: Architecture; required: Architecture };

/**
 * Whether an installed add-on has an upstream update (`UpdateStatus`). Add-on
 * tools ship rolling releases, so `unknown` (network failure / no recorded
 * source) is a normal, non-error result.
 */
export type UpdateStatus =
  | 'current'
  | 'available'
  | 'unknown'
  | 'channel_mismatch'
  | 'unknown_needs_validation';

/**
 * Single freshness verdict a card renders as a status pill (`checking` while a
 * probe is in flight, `available`/`current`/`untracked`/`unknown` otherwise).
 * Every tool derives this identically via {@link deriveFreshness} in
 * `./store-helpers`.
 */
export type Freshness = 'checking' | 'available' | 'untracked' | 'current' | 'unknown';
