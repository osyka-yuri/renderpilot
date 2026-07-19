import {
  defaultHostFacts as sharedDefaultHostFacts,
  isReshadeChannel,
  mapAvailabilitySnapshot,
  type HostDetection,
  type HostFacts,
  type ReshadeChannel,
} from '@entities/addon';

import type { AvailabilityReport, RenoDxActions, RenoDxAddonState } from './types';

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
  return mapAvailabilitySnapshot(report, {
    reshadeStableSupported: report.reshade_stable_supported,
    renodxAddon: report.renodx_addon,
  });
}

export function currentHostChannel(snapshot: AvailabilitySnapshot): ReshadeChannel | null {
  return snapshot.hostFacts.channel.detected;
}

/**
 * Parses a possibly-invalid stored/wire channel value. Availability is not a
 * normalization concern: an explicit unavailable channel must remain visible
 * so callers can disable or reject the corresponding action.
 */
export function normalizeReshadeChannel(
  value: string | null | undefined,
  fallback: ReshadeChannel,
): ReshadeChannel {
  return isReshadeChannel(value) ? value : fallback;
}
