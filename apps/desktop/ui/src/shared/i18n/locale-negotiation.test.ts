import { describe, expect, it } from 'vitest';

import { negotiateLocale } from './locale-negotiation';

describe('negotiateLocale', () => {
  it.each([
    [['EN_us'], 'en'],
    [['ru_RU'], 'ru'],
    [['ES-mx'], 'es'],
    [['fr-CA-u-nu-latn'], 'fr'],
    [['de-Latn-DE'], 'de'],
    [['ja-JP'], 'ja'],
  ] as const)('maps canonical primary languages: %j', (tags, expected) => {
    expect(negotiateLocale(tags)).toBe(expected);
  });

  it('preserves browser preference order while skipping invalid and unsupported tags', () => {
    expect(negotiateLocale(['not_a_tag?', 'pt-BR', 'fr-FR', 'ru-RU'])).toBe('fr');
    expect(negotiateLocale(['pt-BR', 'ru-RU'])).toBe('ru');
    expect(negotiateLocale(['pt-BR', 'it-IT'])).toBe('en');
    expect(negotiateLocale([])).toBe('en');
  });

  it.each([
    ['zh-Hant-CN', 'zh-Hant'],
    ['zh-Hans-TW', 'zh-Hans'],
    ['zh-TW', 'zh-Hant'],
    ['zh-HK', 'zh-Hant'],
    ['zh-MO', 'zh-Hant'],
    ['zh-CN', 'zh-Hans'],
    ['zh-SG', 'zh-Hans'],
    ['zh-MY', 'zh-Hans'],
    ['zh', 'zh-Hans'],
    ['zh-US', 'zh-Hans'],
    ['ZH_hant_hk', 'zh-Hant'],
  ] as const)('applies Chinese script and region rules for %s', (tag, expected) => {
    expect(negotiateLocale([tag])).toBe(expected);
  });
});
