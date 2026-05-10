import { describe, expect, it } from 'vitest';
import {
  normalizeUniqueTrimmedStrings,
  normalizeUniqueTrimmedStringsFromUnknown,
  trimToEmpty,
  trimToOptional,
} from './normalize';

describe('normalize utils', () => {
  describe('trimToEmpty', () => {
    it('trims string values', () => {
      expect(trimToEmpty('  value  ')).toBe('value');
    });

    it('returns empty string for nullish values', () => {
      expect(trimToEmpty(null)).toBe('');
      expect(trimToEmpty(undefined)).toBe('');
    });

    it('returns empty string for blank strings', () => {
      expect(trimToEmpty('   ')).toBe('');
    });
  });

  describe('trimToOptional', () => {
    it('returns trimmed non-empty value', () => {
      expect(trimToOptional('  value  ')).toBe('value');
    });

    it('returns undefined for empty, blank, null and undefined values', () => {
      expect(trimToOptional('')).toBeUndefined();
      expect(trimToOptional('   ')).toBeUndefined();
      expect(trimToOptional(null)).toBeUndefined();
      expect(trimToOptional(undefined)).toBeUndefined();
    });
  });

  describe('normalizeUniqueTrimmedStrings', () => {
    it('trims values, removes empty values and preserves first unique occurrence order', () => {
      expect(normalizeUniqueTrimmedStrings(['  a ', '', ' b ', 'a', '  c  ', 'b'])).toEqual([
        'a',
        'b',
        'c',
      ]);
    });

    it('keeps deduplication case-sensitive', () => {
      expect(normalizeUniqueTrimmedStrings(['Value', ' value ', 'Value'])).toEqual([
        'Value',
        'value',
      ]);
    });
  });

  describe('normalizeUniqueTrimmedStringsFromUnknown', () => {
    it('ignores non-string values', () => {
      expect(
        normalizeUniqueTrimmedStringsFromUnknown([
          '  a ',
          null,
          undefined,
          1,
          false,
          {},
          [],
          'a',
          ' b ',
        ]),
      ).toEqual(['a', 'b']);
    });

    it('returns empty array when no valid string values exist', () => {
      expect(normalizeUniqueTrimmedStringsFromUnknown([null, undefined, 1, false, {}])).toEqual([]);
    });
  });
});
