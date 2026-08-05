import { describe, expect, it } from 'vitest';

import { LAZY_LOCALES, type LazyLocale } from '../../../locale-model';
import { nvapiOverrides as de } from './de';
import { nvapiOverrides as es } from './es';
import { nvapiOverrides as fr } from './fr';
import { nvapiOverrides as ja } from './ja';
import { nvapiOverrides as ru } from './ru';
import { nvapiOverrides as zhHans } from './zh-Hans';
import { nvapiOverrides as zhHant } from './zh-Hant';
import {
  NVAPI_SETTING_COUNT,
  NVAPI_SOURCE_CATALOG,
  NVAPI_VERBATIM_TRANSLATIONS,
} from './contract.generated';

const catalogs: Readonly<Record<LazyLocale, Readonly<Record<string, string>>>> = {
  ru,
  de,
  es,
  fr,
  ja,
  'zh-Hans': zhHans,
  'zh-Hant': zhHant,
};

describe('NVAPI localized catalogs', () => {
  it('contains the exact 17-setting / 170-message bundled snapshot in all seven locales', () => {
    const expectedKeys = Object.keys(NVAPI_SOURCE_CATALOG).toSorted();
    expect(NVAPI_SETTING_COUNT).toBe(17);
    expect(expectedKeys).toHaveLength(170);

    for (const locale of LAZY_LOCALES) {
      const catalog = catalogs[locale];
      expect(Object.keys(catalog).toSorted()).toEqual(expectedKeys);
      for (const translation of Object.values(catalog)) {
        expect(translation.trim().length).toBeGreaterThan(0);
      }
    }
  });

  it('keeps repeated labels and value labels consistent within each locale', () => {
    const keysBySource = Map.groupBy(
      Object.keys(NVAPI_SOURCE_CATALOG),
      (key) => NVAPI_SOURCE_CATALOG[key as keyof typeof NVAPI_SOURCE_CATALOG],
    );

    for (const locale of LAZY_LOCALES) {
      for (const keys of keysBySource.values()) {
        const translations = new Set(keys.map((key) => catalogs[locale][key]));
        expect(translations.size, `${locale}: ${keys.join(', ')}`).toBe(1);
      }
    }
  });

  it('leaves English source text only for policy-approved verbatim values', () => {
    const allowed = new Set(Object.keys(NVAPI_VERBATIM_TRANSLATIONS));
    for (const locale of LAZY_LOCALES) {
      for (const [key, source] of Object.entries(NVAPI_SOURCE_CATALOG)) {
        if (catalogs[locale][key] === source) {
          expect(allowed.has(source), `${locale}: ${key}`).toBe(true);
        }
      }
    }
  });
});
