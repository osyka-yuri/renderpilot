import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import { LAZY_LOCALES, type LazyLocale } from '../locale-model';
import { de } from './de';
import { es } from './es';
import { fr } from './fr';
import { ja } from './ja';
import type { MessageDictionary, MessageValue } from './model';
import { lumaOverrides as lumaDe } from './overrides/luma/de';
import { lumaOverrides as lumaEs } from './overrides/luma/es';
import { lumaOverrides as lumaFr } from './overrides/luma/fr';
import { lumaOverrides as lumaJa } from './overrides/luma/ja';
import { lumaOverrides as lumaRu } from './overrides/luma/ru';
import { lumaOverrides as lumaZhHans } from './overrides/luma/zh-Hans';
import { lumaOverrides as lumaZhHant } from './overrides/luma/zh-Hant';
import { LUMA_SOURCE_CATALOG } from './overrides/luma/schema';
import { nvapiOverrides as nvapiDe } from './overrides/nvapi/de';
import { nvapiOverrides as nvapiEs } from './overrides/nvapi/es';
import { nvapiOverrides as nvapiFr } from './overrides/nvapi/fr';
import { nvapiOverrides as nvapiJa } from './overrides/nvapi/ja';
import { nvapiOverrides as nvapiRu } from './overrides/nvapi/ru';
import { nvapiOverrides as nvapiZhHans } from './overrides/nvapi/zh-Hans';
import { nvapiOverrides as nvapiZhHant } from './overrides/nvapi/zh-Hant';
import { NVAPI_SOURCE_CATALOG } from './overrides/nvapi/contract.generated';
import { ru } from './ru';
import { zhHans } from './zh-Hans';
import { zhHant } from './zh-Hant';

type EditorialPolicy = {
  nvidiaFamilyTerms: Record<
    LazyLocale,
    {
      superResolution: string;
      frameGeneration: string;
      multiFrameGeneration: string;
      rayReconstruction: string;
    }
  >;
  protectedTokens: string[];
  launcherProductNames: Record<'steam' | 'gog' | 'epic' | 'ea' | 'ubisoft', string>;
  nvapiVerbatimValues: string[];
  nvapiSemanticTranslations: Record<LazyLocale, Partial<Record<string, string>>>;
  localeTypography: Record<
    LazyLocale,
    {
      quotationMarks: { open: string; close: string; innerSpacing: boolean };
      forbiddenQuoteMarks: string[];
      forbiddenPunctuation: string[];
      sentenceTerminator: string;
      requiredScript: 'Cyrillic' | 'Japanese' | 'Han' | null;
    }
  >;
};

const policy = JSON.parse(
  readFileSync(new URL('../../../../../data/i18n-editorial-policy.json', import.meta.url), 'utf8'),
) as EditorialPolicy;
const staticCatalogs: Readonly<Record<LazyLocale, MessageDictionary>> = {
  ru,
  de,
  es,
  fr,
  ja,
  'zh-Hans': zhHans,
  'zh-Hant': zhHant,
};
const lumaCatalogs: Readonly<Record<LazyLocale, Readonly<Record<string, string>>>> = {
  ru: lumaRu,
  de: lumaDe,
  es: lumaEs,
  fr: lumaFr,
  ja: lumaJa,
  'zh-Hans': lumaZhHans,
  'zh-Hant': lumaZhHant,
};
const nvapiCatalogs: Readonly<Record<LazyLocale, Readonly<Record<string, string>>>> = {
  ru: nvapiRu,
  de: nvapiDe,
  es: nvapiEs,
  fr: nvapiFr,
  ja: nvapiJa,
  'zh-Hans': nvapiZhHans,
  'zh-Hant': nvapiZhHant,
};

function numericTokens(value: string): string[] {
  return (
    value
      .match(/\d+(?:\.\d+)?(?:\s*%|x| FPS)?/gu)
      ?.map((token) => token.replace(/\s+%$/u, '%'))
      .toSorted() ?? []
  );
}

function copiedCodeTokens(value: string): string[] {
  const patterns = [
    /(?<![\w-])-[a-z][a-z0-9-]*/giu,
    /\b[A-Za-z0-9_-]+\.(?:ini|dll|exe)\b/giu,
    /\[[A-Za-z][A-Za-z0-9]*\]/gu,
    /\b[A-Za-z_][A-Za-z0-9_.]*\s*=\s*(?:true|false|\d+)\b/giu,
    /\bAutoExposure:\s*(?:On|Off)\b/gu,
  ];
  return [...new Set(patterns.flatMap((pattern) => value.match(pattern) ?? []))].toSorted();
}

function staticMessage(catalog: MessageDictionary, key: string): string {
  const message = catalog[key];
  expect(typeof message, key).toBe('string');
  return typeof message === 'string' ? message : '';
}

function occurrences(value: string, token: string): number {
  return value.split(token).length - 1;
}

const protectedTextByLocale = new Map(
  LAZY_LOCALES.map(
    (locale) =>
      [
        locale,
        [
          ...new Set([
            ...policy.protectedTokens,
            ...policy.nvapiVerbatimValues,
            ...Object.values(policy.nvidiaFamilyTerms[locale]),
          ]),
        ].toSorted((left, right) => right.length - left.length || left.localeCompare(right, 'en')),
      ] as const,
  ),
);

function withoutProtectedText(value: string, locale: LazyLocale): string {
  const protectedText = protectedTextByLocale.get(locale);
  if (protectedText === undefined) {
    throw new Error(`Missing protected i18n text for ${locale}`);
  }
  return protectedText.reduce((text, token) => text.replaceAll(token, ' '), value);
}

function templates(value: MessageValue): readonly string[] {
  if (typeof value === 'string') {
    return [value];
  }
  return Object.values(value.kind === 'plural' ? value.forms : value.cases);
}

function authoredTemplates(locale: LazyLocale): readonly (readonly [string, string])[] {
  const staticEntries = Object.entries(staticCatalogs[locale]).flatMap(([key, value]) =>
    templates(value).map((template, index) => [`static.${key}.${index}`, template] as const),
  );
  return [
    ...staticEntries,
    ...Object.entries(lumaCatalogs[locale]).map(([key, value]) => [`luma.${key}`, value] as const),
    ...Object.entries(nvapiCatalogs[locale]).map(
      ([key, value]) => [`nvapi.${key}`, value] as const,
    ),
  ];
}

function englishWordNgrams(value: string, locale: LazyLocale, size: number): string[] {
  const words =
    withoutProtectedText(value, locale)
      .toLocaleLowerCase('en')
      .match(/[a-z]{2,}/gu) ?? [];
  return Array.from({ length: Math.max(0, words.length - size + 1) }, (_, index) =>
    words.slice(index, index + size).join(' '),
  );
}

function containsRequiredScript(
  value: string,
  script: NonNullable<EditorialPolicy['localeTypography'][LazyLocale]['requiredScript']>,
): boolean {
  if (script === 'Cyrillic') {
    return /\p{Script=Cyrillic}/u.test(value);
  }
  if (script === 'Japanese') {
    return /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]/u.test(value);
  }
  return /\p{Script=Han}/u.test(value);
}

describe('external message editorial policy', () => {
  it('preserves protected technical tokens and numeric semantics', () => {
    for (const locale of LAZY_LOCALES) {
      for (const [key, source] of Object.entries({
        ...LUMA_SOURCE_CATALOG,
        ...NVAPI_SOURCE_CATALOG,
      })) {
        const translation = lumaCatalogs[locale][key] ?? nvapiCatalogs[locale][key];
        expect(numericTokens(translation), `${locale}: ${key}`).toEqual(numericTokens(source));
        for (const token of policy.protectedTokens) {
          if (source.includes(token)) {
            expect(translation, `${locale}: ${key}: ${token}`).toContain(token);
          }
        }
      }
    }
  });

  it('preserves copied Luma flags, file names, sections, and assignments verbatim', () => {
    for (const locale of LAZY_LOCALES) {
      for (const [key, source] of Object.entries(LUMA_SOURCE_CATALOG)) {
        expect(copiedCodeTokens(lumaCatalogs[locale][key]), `${locale}: ${key}`).toEqual(
          copiedCodeTokens(source),
        );
      }
    }
  });

  it('uses the official NVIDIA family terms in dynamic and related static namespaces', () => {
    for (const locale of LAZY_LOCALES) {
      const terms = policy.nvidiaFamilyTerms[locale];
      const messages = staticCatalogs[locale];
      expect(messages['settings.nvidia.global.familySr']).toBe(`DLSS ${terms.superResolution}`);
      expect(messages['settings.nvidia.global.familyFg']).toBe(`DLSS ${terms.frameGeneration}`);
      expect(messages['settings.nvidia.global.familyRr']).toBe(`DLSS ${terms.rayReconstruction}`);
      expect(nvapiCatalogs[locale]['nvapi.dlss_sr_render_preset.description']).toContain(
        terms.superResolution,
      );
      expect(nvapiCatalogs[locale]['nvapi.dlss_fg_render_preset.description']).toContain(
        terms.frameGeneration,
      );
      expect(nvapiCatalogs[locale]['nvapi.dlss_mfg_fixed_count.label']).toContain(
        terms.multiFrameGeneration,
      );
      expect(nvapiCatalogs[locale]['nvapi.dlss_rr_render_preset.description']).toContain(
        terms.rayReconstruction,
      );
      expect(staticMessage(messages, 'gameDetails.renodx.component.dlssFixDesc')).toContain(
        terms.frameGeneration,
      );
    }
  });

  it('uses the approved translations for high-risk NVAPI concepts and status values', () => {
    for (const locale of LAZY_LOCALES) {
      const catalog = nvapiCatalogs[locale];
      for (const [key, source] of Object.entries(NVAPI_SOURCE_CATALOG)) {
        const expected = policy.nvapiSemanticTranslations[locale][source];
        if (expected !== undefined) {
          expect(catalog[key], `${locale}: ${key}`).toBe(expected);
        }
      }
    }
  });

  it('preserves launcher product names across the complete launcher namespace', () => {
    for (const locale of LAZY_LOCALES) {
      const catalog = staticCatalogs[locale];
      for (const [key, product] of Object.entries(policy.launcherProductNames)) {
        expect(staticMessage(catalog, `gameDetails.luma.launchArgs.instructions.${key}`)).toContain(
          product,
        );
      }
      expect(
        staticMessage(catalog, 'gameDetails.luma.launchArgs.instructions.other').trim(),
      ).not.toBe('');
    }
  });

  it('uses the locale-specific quotation and punctuation policy', () => {
    for (const locale of LAZY_LOCALES) {
      const typography = policy.localeTypography[locale];
      const { open, close, innerSpacing } = typography.quotationMarks;
      for (const [key, translation] of authoredTemplates(locale)) {
        for (const forbidden of [
          ...typography.forbiddenQuoteMarks,
          ...typography.forbiddenPunctuation,
        ]) {
          expect(translation, `${locale}: ${key}: ${forbidden}`).not.toContain(forbidden);
        }
        expect(occurrences(translation, open), `${locale}: ${key}: opening quotes`).toBe(
          occurrences(translation, close),
        );
        if (translation.includes(open)) {
          const invalidSpacing = innerSpacing
            ? new RegExp(`${open}\\S|\\S${close}`, 'u')
            : new RegExp(`${open}\\s|\\s${close}`, 'u');
          expect(translation, `${locale}: ${key}: quote spacing`).not.toMatch(invalidSpacing);
        }
      }
    }
  });

  it('rejects untranslated source prose with high-confidence locale checks', () => {
    const verbatim = new Set(policy.nvapiVerbatimValues);
    for (const locale of LAZY_LOCALES) {
      const requiredScript = policy.localeTypography[locale].requiredScript;
      for (const [key, source] of Object.entries({
        ...LUMA_SOURCE_CATALOG,
        ...NVAPI_SOURCE_CATALOG,
      })) {
        const translation = lumaCatalogs[locale][key] ?? nvapiCatalogs[locale][key];
        if (verbatim.has(source)) {
          continue;
        }

        expect(translation, `${locale}: ${key}`).not.toBe(source);
        const translatedNgrams = new Set(englishWordNgrams(translation, locale, 3));
        for (const sourceNgram of englishWordNgrams(source, locale, 3)) {
          expect(translatedNgrams.has(sourceNgram), `${locale}: ${key}: ${sourceNgram}`).toBe(
            false,
          );
        }

        if (requiredScript !== null && englishWordNgrams(source, locale, 1).length >= 2) {
          expect(
            containsRequiredScript(withoutProtectedText(translation, locale), requiredScript),
            `${locale}: ${key}: ${requiredScript}`,
          ).toBe(true);
        }
      }
    }
  });
});
