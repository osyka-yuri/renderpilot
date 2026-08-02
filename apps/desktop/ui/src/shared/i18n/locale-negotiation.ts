import { parseLocaleTag } from '@shared/intl';

import { DEFAULT_LOCALE, type Locale } from './locale-model';

const NON_CHINESE_LOCALES: Readonly<Partial<Record<string, Locale>>> = {
  en: 'en',
  ru: 'ru',
  es: 'es',
  fr: 'fr',
  de: 'de',
  ja: 'ja',
};

const TRADITIONAL_CHINESE_REGIONS = new Set(['TW', 'HK', 'MO']);

export function negotiateLocale(languageTags: readonly string[]): Locale {
  for (const languageTag of languageTags) {
    const locale = matchSupportedLocale(languageTag);
    if (locale !== null) {
      return locale;
    }
  }

  return DEFAULT_LOCALE;
}

function matchSupportedLocale(languageTag: string): Locale | null {
  try {
    const parsed = parseLocaleTag(languageTag.replace(/_/gu, '-'));
    if (parsed.language !== 'zh') {
      return NON_CHINESE_LOCALES[parsed.language] ?? null;
    }

    if (parsed.script === 'Hant') {
      return 'zh-Hant';
    }
    if (parsed.script === 'Hans') {
      return 'zh-Hans';
    }

    return parsed.region !== undefined && TRADITIONAL_CHINESE_REGIONS.has(parsed.region)
      ? 'zh-Hant'
      : 'zh-Hans';
  } catch {
    return null;
  }
}
