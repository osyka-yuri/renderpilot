import { describe, expect, it } from 'vitest';

import { canonicalAddonCapabilities } from './addon-capabilities';

describe('canonicalAddonCapabilities', () => {
  it('normalizes, validates, deduplicates, and applies product order', () => {
    expect(canonicalAddonCapabilities([' luma ', 'renodx', 'luma', 'unknown', '  '])).toEqual([
      'renodx',
      'luma',
    ]);
  });
});
