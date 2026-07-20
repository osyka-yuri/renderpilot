import {
  defaultHostFacts as sharedDefaultHostFacts,
  mapAvailabilitySnapshot,
  type ActionDescriptor,
  type HostFacts,
} from '@entities/addon';

import type { AvailabilityReport, LumaActions } from './types';

export type AvailabilitySnapshot = {
  hostDetection: AvailabilityReport['host_detection'];
  hostFacts: HostFacts;
  actions: LumaActions;
  vcredistPresent: boolean | null;
  vcredistInstallerUrl: string;
  installTorn: boolean;
};

export type AvailabilitySnapshotSource = Pick<
  AvailabilityReport,
  | 'host_detection'
  | 'host_facts'
  | 'actions'
  | 'vcredist_present'
  | 'vcredist_installer_url'
  | 'install_torn'
>;

/** Luma always installs the nightly ReShade host — there is no channel switch. */
export function defaultHostFacts(): HostFacts {
  return sharedDefaultHostFacts('nightly');
}

export function availabilitySnapshotFromReport(
  report: AvailabilitySnapshotSource,
): AvailabilitySnapshot {
  return mapAvailabilitySnapshot(report, {
    vcredistPresent: report.vcredist_present,
    vcredistInstallerUrl: report.vcredist_installer_url,
    installTorn: report.install_torn,
  });
}

/** Payload Repair is actionable only when the torn install still has a live profile. */
export function payloadRepairAction(
  installTorn: boolean,
  isInstallable: boolean,
): ActionDescriptor | undefined {
  if (!installTorn || !isInstallable) {
    return undefined;
  }
  return {
    enabled: true,
    requires_confirmation: false,
    confirmation_scope: null,
    disabled_reason: null,
    target_channel: null,
  };
}
