import { describe, expect, it } from 'vitest';

import { parseHttpDateTimestamp, parseRfc3339Timestamp } from './parse';

describe('strict date parsers', () => {
  describe('parseRfc3339Timestamp', () => {
    it('accepts absolute timestamps, offsets, fractions and a real leap day', () => {
      expect(parseRfc3339Timestamp('2026-07-11T12:00:00Z')).toBe(Date.UTC(2026, 6, 11, 12));
      expect(parseRfc3339Timestamp('2026-07-11T12:00:00+02:30')).toBe(Date.UTC(2026, 6, 11, 9, 30));
      expect(parseRfc3339Timestamp('2026-07-11T12:00:00.123456Z')).toBe(
        Date.UTC(2026, 6, 11, 12, 0, 0, 123),
      );
      expect(parseRfc3339Timestamp('2024-02-29t00:00:00z')).toBe(Date.UTC(2024, 1, 29));
    });

    it.each([
      null,
      undefined,
      '',
      '2026-02-30T00:00:00Z',
      '2025-02-29T00:00:00Z',
      '2026-07-11',
      '2026-07-11T12:00:00',
      '01/02/2026',
      '2026-07-11T24:00:00Z',
      '2026-07-11T12:00:00+24:00',
      'not-a-date',
    ])('rejects non-RFC3339 or impossible input: %s', (value) => {
      expect(parseRfc3339Timestamp(value)).toBeNull();
    });
  });

  describe('parseHttpDateTimestamp', () => {
    it('accepts canonical IMF-fixdate', () => {
      expect(parseHttpDateTimestamp('Wed, 18 Jun 2025 12:00:00 GMT')).toBe(
        Date.UTC(2025, 5, 18, 12),
      );
    });

    it.each([
      null,
      undefined,
      '',
      'Thu, 18 Jun 2025 12:00:00 GMT',
      'Mon, 31 Feb 2025 12:00:00 GMT',
      'Wednesday, 18-Jun-25 12:00:00 GMT',
      'Wed Jun 18 12:00:00 2025',
      '2025-06-18T12:00:00Z',
      'Wed, 18 Jun 2025 24:00:00 GMT',
    ])('rejects non-canonical or impossible input: %s', (value) => {
      expect(parseHttpDateTimestamp(value)).toBeNull();
    });
  });
});
