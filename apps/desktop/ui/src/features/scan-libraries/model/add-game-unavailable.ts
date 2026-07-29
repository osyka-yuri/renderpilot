import { t, type MessageKey } from '@shared/i18n';
import type { AddGameUnavailableReason } from './add-game';

const messageKeyByReason: Readonly<Record<AddGameUnavailableReason, MessageKey>> = {
  multiple_installs: 'addGame.unavailable.multipleInstalls',
  contains_proven_install: 'addGame.unavailable.containsProvenInstall',
  contains_multiple_catalog_installs: 'addGame.unavailable.containsMultipleCatalogInstalls',
  inside_existing_install: 'addGame.unavailable.insideExistingInstall',
  no_readable_executable: 'addGame.unavailable.noReadableExecutable',
  root_correction_blocked: 'addGame.unavailable.rootCorrectionBlocked',
};

const warningCodesByReason: Readonly<Partial<Record<AddGameUnavailableReason, readonly string[]>>> =
  {
    multiple_installs: ['multiple_installs_suspected', 'contains_proven_install'],
    contains_proven_install: ['contains_proven_install'],
    contains_multiple_catalog_installs: [
      'multiple_installs_suspected',
      'multiple_proven_installs',
      'contains_proven_install',
    ],
    inside_existing_install: ['inside_existing_install', 'narrows_existing_install'],
    no_readable_executable: ['no_readable_executable'],
  };

/** Formats one backend-owned unavailable reason in the active UI locale. */
export function formatAddGameUnavailableReason(reason: AddGameUnavailableReason): string {
  return t(messageKeyByReason[reason]);
}

/** Whether an unavailable reason already communicates the given inspection warning. */
export function addGameUnavailableReasonCoversWarning(
  reason: AddGameUnavailableReason,
  warningCode: string,
): boolean {
  return warningCodesByReason[reason]?.includes(warningCode) ?? false;
}
