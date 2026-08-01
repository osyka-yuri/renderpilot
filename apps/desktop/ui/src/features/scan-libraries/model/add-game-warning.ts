import { t, type MessageKeyWithoutParams } from '@shared/i18n';

import type { AddGameWarning } from './add-game';

const plainWarningKeyByCode: Readonly<Partial<Record<string, MessageKeyWithoutParams>>> = {
  unsupported_platform: 'addGame.warning.unsupportedPlatform',
  probe_incomplete: 'addGame.warning.probeIncomplete',
  parent_probe_incomplete: 'addGame.warning.parentProbeIncomplete',
  inside_existing_install: 'addGame.warning.insideExistingInstall',
  narrows_existing_install: 'addGame.warning.narrowsExistingInstall',
  multiple_proven_installs: 'addGame.warning.multipleProvenInstalls',
  contains_proven_install: 'addGame.warning.containsProvenInstall',
  multiple_installs_suspected: 'addGame.warning.multipleInstallsSuspected',
  explicit_executable_required: 'addGame.warning.explicitExecutableRequired',
  no_readable_executable: 'addGame.warning.noReadableExecutable',
  filesystem_probe_error: 'addGame.warning.filesystemProbeError',
};

function numericParameter(warning: AddGameWarning, name: string): number | null {
  const value = warning.parameters?.[name];
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function interpolationParameter(warning: AddGameWarning, name: string): string | number | null {
  const value = warning.parameters?.[name];
  return typeof value === 'string' || (typeof value === 'number' && Number.isFinite(value))
    ? value
    : null;
}

/** Formats a structured backend warning in the active UI locale. */
export function formatAddGameWarning(warning: AddGameWarning): string {
  switch (warning.code) {
    case 'legacy_cards_consolidated': {
      const count = numericParameter(warning, 'count');
      return count === null
        ? warning.message
        : t('addGame.warning.legacyCardsConsolidated', { count });
    }
    case 'legacy_cards_retained': {
      const count = numericParameter(warning, 'count');
      return count === null ? warning.message : t('addGame.warning.legacyCardsRetained', { count });
    }
    case 'recovery_bundle_created': {
      const path = interpolationParameter(warning, 'path');
      return path === null ? warning.message : t('addGame.warning.recoveryBundleCreated', { path });
    }
    case 'root_correction_history_archived': {
      const path = interpolationParameter(warning, 'path');
      return path === null
        ? warning.message
        : t('addGame.warning.rootCorrectionHistoryArchived', { path });
    }
    default: {
      const key = plainWarningKeyByCode[warning.code];
      return key === undefined ? warning.message : t(key);
    }
  }
}
