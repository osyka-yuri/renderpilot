import { describe, expect, it } from 'vitest';

import { LAZY_LOCALES, type LazyLocale } from '../locale-model';
import {
  expandLumaTranslations,
  LUMA_MESSAGE_GROUPS,
  type LumaMessageKey,
  type LumaMessagePhrase,
  type LumaMessageTranslations,
} from './overrides/luma/schema';
import { lumaOverrides as de } from './overrides/luma/de';
import { lumaOverrides as es } from './overrides/luma/es';
import { lumaOverrides as fr } from './overrides/luma/fr';
import { lumaOverrides as ja } from './overrides/luma/ja';
import { lumaOverrides as ru } from './overrides/luma/ru';
import { lumaOverrides as zhHans } from './overrides/luma/zh-Hans';
import { lumaOverrides as zhHant } from './overrides/luma/zh-Hant';

const lumaCatalogs = {
  ru,
  de,
  es,
  fr,
  ja,
  'zh-Hans': zhHans,
  'zh-Hant': zhHant,
} as const satisfies Readonly<Record<LazyLocale, Readonly<Record<string, string>>>>;
const lumaMessageKeys = Object.values(LUMA_MESSAGE_GROUPS).flat();
const lumaMessageGroups = Object.entries(LUMA_MESSAGE_GROUPS) as [
  LumaMessagePhrase,
  readonly LumaMessageKey[],
][];

describe('Luma message catalogs', () => {
  it('expands every current external message ID exactly once for each translated locale', () => {
    const expectedKeys = lumaMessageKeys.toSorted();

    expect(new Set(lumaMessageKeys).size).toBe(lumaMessageKeys.length);
    expect(lumaMessageKeys).toHaveLength(104);

    for (const locale of LAZY_LOCALES) {
      const overrides = lumaCatalogs[locale];

      expect(overrides).toBeDefined();
      expect(Object.keys(overrides).sort()).toEqual(expectedKeys);
      for (const translation of Object.values(overrides)) {
        expect(translation.trim().length).toBeGreaterThan(0);
      }
    }
  });

  it('maps every key to its phrase value in deterministic group order', () => {
    const translations = Object.fromEntries(
      lumaMessageGroups.map(([phrase]) => [phrase, `translation:${phrase}`]),
    ) as LumaMessageTranslations;

    const first = expandLumaTranslations(translations);
    const second = expandLumaTranslations(translations);

    expect(Object.keys(first)).toEqual(lumaMessageKeys);
    expect(second).toEqual(first);
    for (const [phrase, ids] of lumaMessageGroups) {
      for (const id of ids) {
        expect(first[id]).toBe(`translation:${phrase}`);
      }
    }
  });
});
