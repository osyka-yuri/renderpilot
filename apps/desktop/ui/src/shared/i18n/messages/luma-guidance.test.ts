import { describe, expect, it } from 'vitest';

import {
  expandLumaGuidanceTranslations,
  LUMA_GUIDANCE_GROUPS,
  type LumaGuidanceKey,
  type LumaGuidancePhrase,
  type LumaGuidanceTranslations,
} from './overrides/luma/schema';
import { lumaGuidanceOverrides as de } from './overrides/luma/de';
import { lumaGuidanceOverrides as es } from './overrides/luma/es';
import { lumaGuidanceOverrides as fr } from './overrides/luma/fr';
import { lumaGuidanceOverrides as ja } from './overrides/luma/ja';
import { lumaGuidanceOverrides as ru } from './overrides/luma/ru';
import { lumaGuidanceOverrides as zh } from './overrides/luma/zh';

const nonEnglishLocales = ['ru', 'de', 'es', 'fr', 'ja', 'zh'] as const;
const lumaGuidanceOverrides = { ru, de, es, fr, ja, zh } as const;
const lumaGuidanceKeys = Object.values(LUMA_GUIDANCE_GROUPS).flat();
const lumaGuidanceGroups = Object.entries(LUMA_GUIDANCE_GROUPS) as [
  LumaGuidancePhrase,
  readonly LumaGuidanceKey[],
][];

describe('lumaGuidanceOverrides', () => {
  it('expands every current guidance ID exactly once for each translated locale', () => {
    const expectedKeys = [...lumaGuidanceKeys].sort();

    // Uniqueness is the contract; length is free to grow with the catalogue.
    expect(new Set(lumaGuidanceKeys).size).toBe(lumaGuidanceKeys.length);
    expect(lumaGuidanceKeys.length).toBeGreaterThan(0);

    for (const locale of nonEnglishLocales) {
      const overrides = lumaGuidanceOverrides[locale];

      expect(overrides).toBeDefined();
      expect(Object.keys(overrides).sort()).toEqual(expectedKeys);
      for (const translation of Object.values(overrides)) {
        expect(translation.trim().length).toBeGreaterThan(0);
      }
    }
  });

  it('maps every key to its phrase value in deterministic group order', () => {
    const translations = Object.fromEntries(
      lumaGuidanceGroups.map(([phrase]) => [phrase, `translation:${phrase}`]),
    ) as LumaGuidanceTranslations;

    const first = expandLumaGuidanceTranslations(translations);
    const second = expandLumaGuidanceTranslations(translations);

    expect(Object.keys(first)).toEqual(lumaGuidanceKeys);
    expect(second).toEqual(first);
    for (const [phrase, ids] of lumaGuidanceGroups) {
      for (const id of ids) {
        expect(first[id]).toBe(`translation:${phrase}`);
      }
    }
  });
});
