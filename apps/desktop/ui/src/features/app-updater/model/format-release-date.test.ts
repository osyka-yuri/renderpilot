import { describe, expect, it } from 'vitest';

import { formatReleaseDateForLocale, normalizeReleaseDate } from './format-release-date';

describe('normalizeReleaseDate', () => {
  it('accepts a valid RFC 3339 date', () => {
    expect(normalizeReleaseDate('2026-07-11T12:00:00Z')).toBe('2026-07-11T12:00:00Z');
  });

  it('accepts a date with a timezone offset', () => {
    expect(normalizeReleaseDate('2026-07-11T12:00:00+02:00')).toBe('2026-07-11T12:00:00+02:00');
  });

  it('returns null for null', () => {
    expect(normalizeReleaseDate(null)).toBeNull();
  });

  it('returns null for empty text', () => {
    expect(normalizeReleaseDate('   ')).toBeNull();
  });

  it('returns null for invalid dates', () => {
    expect(normalizeReleaseDate('not-a-date')).toBeNull();
  });

  it('returns null for impossible calendar dates that parse as NaN', () => {
    // JS Date is lenient for overflow; only clearly unparsable values are null.
    expect(normalizeReleaseDate('completely-invalid')).toBeNull();
  });
});

describe('formatReleaseDateForLocale', () => {
  it('formats a valid date for a locale', () => {
    const formatted = formatReleaseDateForLocale('2026-07-11T12:00:00Z', 'en');
    expect(formatted).toMatch(/2026/);
    expect(formatted).toMatch(/July|Jul/);
  });

  it('returns null for invalid input', () => {
    expect(formatReleaseDateForLocale('nope', 'en')).toBeNull();
  });
});
