import { describe, expect, it } from 'vitest';

import {
  formatLocalDateTime,
  formatLocalShortDate,
  formatRelativeTime,
  formatUtcLongDate,
  formatUtcNumericDate,
  formatUtcShortDate,
} from './date-time';

describe('date-time formatters', () => {
  it('formats local calendar dates and timestamps', () => {
    const value = Date.UTC(2026, 6, 11, 12);

    expect(formatLocalShortDate(value, 'en')).toMatch(/2026/u);
    expect(formatLocalDateTime(value, 'en')).toMatch(/2026/u);
  });

  it('uses UTC for metadata and prevents local date shifts', () => {
    const value = Date.UTC(2025, 11, 31, 22, 30);

    expect(formatUtcShortDate(value, 'en')).toBe('Dec 31, 2025');
    expect(formatUtcLongDate(value, 'en')).toBe('December 31, 2025');
    expect(formatUtcNumericDate(value, 'ru')).toBe('31.12.2025');
  });

  it('formats relative thresholds using an injected clock', () => {
    const now = Date.UTC(2026, 6, 11, 12);

    expect(formatRelativeTime(now, 'en', now)).toBe('now');
    expect(formatRelativeTime(now - 59_000, 'en', now)).toBe('59 seconds ago');
    expect(formatRelativeTime(now - 60_000, 'en', now)).toBe('1 minute ago');
    expect(formatRelativeTime(now - 120_000, 'en', now)).toBe('2 minutes ago');
    expect(formatRelativeTime(now + 3_600_000, 'ja', now)).toBe('1 時間後');
  });

  it('returns null for invalid dates and timestamps', () => {
    expect(formatLocalShortDate(Number.NaN, 'en')).toBeNull();
    expect(formatLocalShortDate(Number.MAX_VALUE, 'en')).toBeNull();
    expect(formatRelativeTime(Number.NaN, 'en')).toBeNull();
    expect(formatRelativeTime(Number.MAX_VALUE, 'en')).toBeNull();
  });
});
