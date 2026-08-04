import { describe, expect, it } from 'vitest';

import { t } from '@shared/i18n';
import { formatCompactDurationSeconds } from '@shared/format';

import { getCompletedDurationText } from './presenters';

describe('getCompletedDurationText', () => {
  it.each([
    [Number.NaN, 1_000],
    [0, Number.NaN],
    [Number.POSITIVE_INFINITY, 1_000],
    [0, Number.NEGATIVE_INFINITY],
  ])('rejects invalid timestamps', (createdAt, completedAt) => {
    expect(getCompletedDurationText(createdAt, completedAt, 'en')).toBeNull();
  });

  it('returns null while an operation is incomplete', () => {
    expect(getCompletedDurationText(0, null, 'en')).toBeNull();
  });

  it('clamps a negative delta to zero', () => {
    expect(getCompletedDurationText(2_000, 1_000, 'en')).toBe(
      t('operation.duration', { duration: '0s' }),
    );
  });

  it.each([
    [0, '0s'],
    [59_000, '59s'],
    [60_000, '1m 0s'],
    [3_661_000, '1h 1m 1s'],
    [90_061_000, '1d 1h 1m 1s'],
  ] as const)('formats a %i millisecond delta', (completedAt, duration) => {
    expect(getCompletedDurationText(0, completedAt, 'en')).toBe(
      t('operation.duration', { duration }),
    );
  });

  it('rejects durations that cannot be represented as safe integer seconds', () => {
    expect(getCompletedDurationText(0, Number.MAX_SAFE_INTEGER * 2_000, 'en')).toBeNull();
  });

  it('formats the duration in the requested locale', () => {
    const duration = formatCompactDurationSeconds(3661, 'ru');

    expect(duration).not.toBeNull();
    if (duration === null) {
      throw new Error('Expected a Russian duration');
    }
    expect(getCompletedDurationText(0, 3_661_000, 'ru')).toBe(
      t('operation.duration', { duration }),
    );
  });
});
