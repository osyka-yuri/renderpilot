import { describe, expect, it } from 'vitest';
import { t } from '@shared/i18n';
import { formatPartialScanWarning } from './scan-presenters';

type PartialScanWarningCase = {
  name: string;
  scanErrorCount: number;
};

const partialScanWarningCases = [
  { name: 'single failed root', scanErrorCount: 1 },
  { name: 'two failed roots', scanErrorCount: 2 },
  { name: 'many failed roots', scanErrorCount: 10 },
] satisfies readonly PartialScanWarningCase[];

const invalidScanErrorCounts = [
  0,
  -0,
  -1,
  1.5,
  Number.NaN,
  Number.POSITIVE_INFINITY,
  Number.NEGATIVE_INFINITY,
  Number.MAX_SAFE_INTEGER + 1,
] as const;

describe('scan-presenters', () => {
  describe('formatPartialScanWarning', () => {
    it.each(partialScanWarningCases)('$name', ({ scanErrorCount }) => {
      expect(formatPartialScanWarning(scanErrorCount)).toBe(
        t('scan.partialWarning', { count: scanErrorCount }),
      );
    });

    it.each(invalidScanErrorCounts)(
      'throws RangeError for invalid scan error count: %s',
      (scanErrorCount) => {
        expect(() => formatPartialScanWarning(scanErrorCount)).toThrow(RangeError);
        expect(() => formatPartialScanWarning(scanErrorCount)).toThrow(
          'scanErrorCount must be a positive integer.',
        );
      },
    );
  });
});
