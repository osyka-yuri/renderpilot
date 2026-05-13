import { describe, expect, it } from 'vitest';

import { compactList } from './list';

describe('list utils', () => {
  describe('compactList', () => {
    it('returns empty copy for empty list', () => {
      expect(compactList([], 'None')).toBe('None');
    });

    it('joins items with dot separator', () => {
      expect(compactList(['A', 'B'], 'None')).toBe('A · B');
    });

    it('truncates after default limit of 3', () => {
      expect(compactList(['A', 'B', 'C', 'D'], 'None')).toBe('A · B · C +1 more');
    });

    it('respects custom limit', () => {
      expect(compactList(['A', 'B', 'C'], 'None', 2)).toBe('A · B +1 more');
    });
  });
});
