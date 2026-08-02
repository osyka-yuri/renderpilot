import type { LocaleLoadError } from './errors';
import type { InterpolationParams } from './messages/model';
import type { LanguageMode, Locale } from './locale-model';
import type { LocaleLoader, LocalePack } from './packs/types';

export type I18nInitializationResult = Readonly<{
  activeMode: LanguageMode;
  activeLocale: Locale;
  fallbackUsed: boolean;
  error: LocaleLoadError | null;
}>;

export type I18nSwitchResult =
  | Readonly<{ outcome: 'applied'; mode: LanguageMode; locale: Locale }>
  | Readonly<{ outcome: 'superseded'; mode: LanguageMode; locale: Locale }>;

export type I18nRuntimeState = Readonly<{
  status: 'idle' | 'loading' | 'ready' | 'error';
  activeMode: LanguageMode;
  activeLocale: Locale;
  pending: Readonly<{ mode: LanguageMode; locale: Locale }> | null;
  error: LocaleLoadError | null;
}>;

export type I18nRuntimeDependencies = Readonly<{
  fallbackPack: LocalePack;
  loaders: Readonly<Record<Locale, LocaleLoader>>;
  readStoredMode: () => LanguageMode;
  persistMode: (mode: LanguageMode) => void;
  resolveMode: (mode: LanguageMode) => Locale;
  observeSystemLanguage: (listener: () => void) => () => void;
  applyDocumentLocale: (locale: Locale) => void;
}>;

export type ExternalMessageInput = Readonly<{
  key: string;
  fallback: string;
  params?: InterpolationParams;
}>;
