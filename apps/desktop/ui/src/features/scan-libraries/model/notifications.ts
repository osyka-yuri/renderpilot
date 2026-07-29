import { formatPartialScanWarning } from '@entities/game';
import { t } from '@shared/i18n';
import { publishStatusNotification } from '@shared/notifications';
import type { AddGameResult } from './add-game';
import { formatAddGameWarning } from './add-game-warning';

export function publishAutomaticLibraryScanFailedNotification(message: string): string | null {
  return publishStatusNotification(message, 'error');
}

export function publishPartialLibraryScanWarning(scanErrorCount: number): string | null {
  return publishStatusNotification(formatPartialScanWarning(scanErrorCount), 'warning');
}

export function publishAddGameWarnings(result: AddGameResult): string | null {
  const messages = result.warnings
    .map(formatAddGameWarning)
    .map((message) => message.trim())
    .filter(Boolean);
  const recoveryBundlePath = result.recoveryBundlePath;
  if (
    recoveryBundlePath !== null &&
    !result.warnings.some((warning) => warning.code === 'recovery_bundle_created')
  ) {
    messages.push(t('addGame.warning.recoveryBundleFallback', { path: recoveryBundlePath }));
  }
  return messages.length === 0 ? null : publishStatusNotification(messages.join('\n'), 'warning');
}
