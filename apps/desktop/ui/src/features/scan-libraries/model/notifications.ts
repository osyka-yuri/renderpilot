import { formatPartialScanWarning } from '@entities/game';
import { publishStatusNotification } from '@shared/notifications';

export function publishAutomaticLibraryScanFailedNotification(message: string): string | null {
  return publishStatusNotification(message, 'error');
}

export function publishPartialLibraryScanWarning(scanErrorCount: number): string | null {
  return publishStatusNotification(formatPartialScanWarning(scanErrorCount), 'warning');
}