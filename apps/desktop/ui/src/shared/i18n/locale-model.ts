export const LOCALES = ['en', 'ru', 'es', 'fr', 'de', 'ja', 'zh-Hans', 'zh-Hant'] as const;
export type Locale = (typeof LOCALES)[number];

export const LAZY_LOCALES = [
  'ru',
  'es',
  'fr',
  'de',
  'ja',
  'zh-Hans',
  'zh-Hant',
] as const satisfies readonly Locale[];
export type LazyLocale = (typeof LAZY_LOCALES)[number];

export const LANGUAGE_MODES = ['system', ...LOCALES] as const;
export type LanguageMode = (typeof LANGUAGE_MODES)[number];

export const DEFAULT_LANGUAGE_MODE: LanguageMode = 'system';
export const DEFAULT_LOCALE: Locale = 'en';

export function isLanguageMode(value: unknown): value is LanguageMode {
  return typeof value === 'string' && LANGUAGE_MODES.includes(value as LanguageMode);
}
