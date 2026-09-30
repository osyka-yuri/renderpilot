import {
  defaultHostFacts as sharedDefaultHostFacts,
  isReshadeChannel,
  mapAvailabilitySnapshot,
  type HostDetection,
  type HostFacts,
  type ReshadeChannel,
} from '@entities/addon';

import type { AvailabilityReport, RenoDxActions, RenoDxAddonState } from './types';
import type { EngineConfigAvailability } from '@entities/addon';

const DEFAULT_ENGINE_CONFIG: EngineConfigAvailability = {
  status: 'not_applicable',
  path: null,
  can_apply: false,
};

export type AvailabilitySnapshot = {
  hasPersistedRecord: boolean;
  engineConfig: NonNullable<AvailabilityReport['engine_config']>;
  hostDetection: HostDetection;
  hostFacts: HostFacts;
  actions: RenoDxActions;
  reshadeStableSupported: boolean;
  renodxAddon: RenoDxAddonState | null;
  installTorn: boolean;
};

export type AvailabilitySnapshotSource = Pick<
  AvailabilityReport,
  | 'engine_config'
  | 'host_detection'
  | 'host_facts'
  | 'actions'
  | 'reshade_stable_supported'
  | 'renodx_addon'
  | 'install_torn'
  | 'has_persisted_record'
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
    engineConfig: report.engine_config ?? DEFAULT_ENGINE_CONFIG,
    reshadeStableSupported: report.reshade_stable_supported,
    renodxAddon: report.renodx_addon,
    installTorn: report.install_torn,
    hasPersistedRecord: report.has_persisted_record,
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
