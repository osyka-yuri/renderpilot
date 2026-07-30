import { describe, expect, it } from 'vitest';

import { formatPercent } from './numbers';

describe('formatPercent', () => {
  it('formats a ratio with locale-specific percent spacing', () => {
    expect(formatPercent(0.6, 'en')).toBe('60%');
    expect(formatPercent(0.6, 'ru')).toMatch(/^60\s%$/u);
    expect(formatPercent(0.6, 'fr')).toMatch(/^60\s%$/u);
  });

  it('clamps ratios and normalizes non-finite values', () => {
    expect(formatPercent(-1, 'en')).toBe('0%');
    expect(formatPercent(2, 'en')).toBe('100%');
    expect(formatPercent(Number.NaN, 'en')).toBe('0%');
  });
});
