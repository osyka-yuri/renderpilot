import { t } from '@shared/i18n';

export function formatPartialScanWarning(scanErrorCount: number): string {
  assertPositiveInteger(scanErrorCount, 'scanErrorCount');

  return t('scan.partialWarning', { count: scanErrorCount });
}

function assertPositiveInteger(value: number, name: string): void {
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new RangeError(`${name} must be a positive integer.`);
  }
}
