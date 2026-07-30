import { describe, expect, it } from 'vitest';

import { formatList } from './list';

describe('formatList', () => {
  it('returns an empty string for an empty list', () => {
    expect(formatList([], 'en')).toBe('');
  });

  it('uses locale-specific conjunctions and punctuation', () => {
    expect(formatList(['A', 'B', 'C'], 'en')).toBe('A, B, and C');
    expect(formatList(['A', 'B', 'C'], 'ru')).toBe('A, B и C');
    expect(formatList(['A', 'B', 'C'], 'ja')).toBe('A、B、C');
  });
});
