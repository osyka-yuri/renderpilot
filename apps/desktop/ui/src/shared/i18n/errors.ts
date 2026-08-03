import type { LanguageMode, Locale } from './locale-model';

export class LocaleLoadError extends Error {
  readonly code = 'i18n_locale_load_failed';
  readonly mode: LanguageMode;
  readonly locale: Locale;

  constructor(mode: LanguageMode, locale: Locale, cause: unknown) {
    super('i18n_locale_load_failed', { cause });
    this.name = 'LocaleLoadError';
    this.mode = mode;
    this.locale = locale;
  }
}
