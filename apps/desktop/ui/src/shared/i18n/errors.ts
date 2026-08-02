import type { LanguageMode, Locale } from './locale-model';

export class LocaleLoadError extends Error {
  readonly code = 'i18n_locale_load_failed';
  readonly mode: LanguageMode;
  readonly locale: Locale;
  readonly cause: unknown;

  constructor(mode: LanguageMode, locale: Locale, cause: unknown) {
    super(`Failed to load locale pack "${locale}".`);
    this.name = 'LocaleLoadError';
    this.mode = mode;
    this.locale = locale;
    this.cause = cause;
  }
}
