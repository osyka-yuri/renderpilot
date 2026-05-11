import { describe, expect, it } from 'vitest';
import { formatPartialScanWarning } from './scan-presenters';

type PartialScanWarningCase = {
  name: string;
  scanErrorCount: number;
  expected: string;
};

const partialScanWarningCases = [
  {
    name: 'single failed root',
    scanErrorCount: 1,
    expected: 'Some game libraries could not be scanned (1 root failed). Check logs for details.',
  },
  {
    name: 'two failed roots',
    scanErrorCount: 2,
    expected: 'Some game libraries could not be scanned (2 roots failed). Check logs for details.',
  },
  {
    name: 'many failed roots',
    scanErrorCount: 10,
    expected: 'Some game libraries could not be scanned (10 roots failed). Check logs for details.',
  },
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
    it.each(partialScanWarningCases)('$name', ({ scanErrorCount, expected }) => {
      expect(formatPartialScanWarning(scanErrorCount)).toBe(expected);
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
