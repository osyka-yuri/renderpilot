import { describe, expect, it } from 'vitest';

import { formatCompactDurationSeconds } from './duration';

describe('formatCompactDurationSeconds', () => {
  it.each([Number.NaN, Number.POSITIVE_INFINITY, -1, 1.5, Number.MAX_SAFE_INTEGER + 1])(
    'rejects invalid seconds: %s',
    (seconds) => {
      expect(formatCompactDurationSeconds(seconds, 'en')).toBeNull();
    },
  );

  it.each([
    [0, '0s'],
    [59, '59s'],
    [60, '1m 0s'],
    [3661, '1h 1m 1s'],
    [90_061, '1d 1h 1m 1s'],
  ] as const)('formats %i seconds in English', (seconds, expected) => {
    expect(formatCompactDurationSeconds(seconds, 'en')).toBe(expected);
  });

  it('uses locale-specific unit labels', () => {
    expect(formatCompactDurationSeconds(3661, 'ru')).toMatch(/1\s*ч.*1\s*мин.*1\s*с/u);
  });
});
