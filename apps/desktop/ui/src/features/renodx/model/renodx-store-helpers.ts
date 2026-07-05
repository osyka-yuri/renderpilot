import { defaultHostFacts as sharedDefaultHostFacts, deriveFreshness } from '@entities/addon';

import type {
  AvailabilityReport,
  HostDetection,
  HostFacts,
  RenoDxActions,
  RenoDxAddonState,
  ReshadeChannel,
} from './types';

export type AvailabilitySnapshot = {
  hostDetection: HostDetection;
  hostFacts: HostFacts;
  actions: RenoDxActions;
  reshadeStableSupported: boolean;
  renodxAddon: RenoDxAddonState | null;
};

export type AvailabilitySnapshotSource = Pick<
  AvailabilityReport,
  'host_detection' | 'host_facts' | 'actions' | 'reshade_stable_supported' | 'renodx_addon'
>;

/** RenoDX defaults to the stable ReShade channel until an availability report
 * says otherwise. */
export function defaultHostFacts(): HostFacts {
  return sharedDefaultHostFacts('stable');
}

export function availabilitySnapshotFromReport(
  report: AvailabilitySnapshotSource,
): AvailabilitySnapshot {
  return {
    hostDetection: report.host_detection,
    hostFacts: report.host_facts,
    actions: report.actions,
    reshadeStableSupported: report.reshade_stable_supported,
    renodxAddon: report.renodx_addon,
  };
}

export function currentHostChannel(snapshot: AvailabilitySnapshot): ReshadeChannel | null {
  return snapshot.hostFacts.channel.detected;
}

/**
 * Falls a channel back to nightly when it's `stable` but the manifest/report
 * doesn't offer a stable channel. Shared by every store that tracks a
 * user-selected ReShade channel.
 */
export function degradeUnsupportedStableChannel(
  channel: ReshadeChannel,
  stableSupported: boolean,
): ReshadeChannel {
  return channel === 'stable' && !stableSupported ? 'nightly' : channel;
}

/**
 * Parses a possibly-invalid stored/wire channel value, falling back to
 * `fallback` when absent or unrecognized, then applies the same
 * stable-unsupported degrade as {@link degradeUnsupportedStableChannel}.
 */
export function normalizeReshadeChannel(
  value: string | null | undefined,
  fallback: ReshadeChannel,
  stableSupported: boolean,
): ReshadeChannel {
  const parsed = value === 'nightly' ? 'nightly' : value === 'stable' ? 'stable' : fallback;
  return degradeUnsupportedStableChannel(parsed, stableSupported);
}

export { deriveFreshness };
