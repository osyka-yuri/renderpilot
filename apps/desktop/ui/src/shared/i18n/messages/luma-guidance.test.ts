import { describe, expect, it } from 'vitest';

import { lumaGuidanceKeys, lumaGuidanceOverrides } from './luma-guidance';

const nonEnglishLocales = ['ru', 'de', 'es', 'fr', 'ja', 'zh'] as const;

describe('lumaGuidanceOverrides', () => {
  it('expands every current guidance ID exactly once for each translated locale', () => {
    const expectedKeys = [...lumaGuidanceKeys].sort();

    // Uniqueness is the contract; length is free to grow with the catalogue.
    expect(new Set(lumaGuidanceKeys).size).toBe(lumaGuidanceKeys.length);
    expect(lumaGuidanceKeys.length).toBeGreaterThan(0);

    for (const locale of nonEnglishLocales) {
      const overrides = lumaGuidanceOverrides[locale];

      expect(overrides).toBeDefined();
      expect(Object.keys(overrides ?? {}).sort()).toEqual(expectedKeys);
      for (const translation of Object.values(overrides ?? {})) {
        expect(translation.trim().length).toBeGreaterThan(0);
      }
    }
  });
});
