import { t, type MessageKey } from '@shared/i18n';
import type { AddGameWarning } from './add-game';

const warningKeyByCode: Readonly<Partial<Record<string, MessageKey>>> = {
  legacy_cards_consolidated: 'addGame.warning.legacyCardsConsolidated',
  legacy_cards_retained: 'addGame.warning.legacyCardsRetained',
  recovery_bundle_created: 'addGame.warning.recoveryBundleCreated',
  root_correction_history_archived: 'addGame.warning.rootCorrectionHistoryArchived',
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

/** Formats a structured backend warning in the active UI locale. */
export function formatAddGameWarning(warning: AddGameWarning): string {
  const key = warningKeyByCode[warning.code];
  return key === undefined ? warning.message : t(key, warning.parameters);
}
