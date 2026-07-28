export type { Locale, LanguageMode } from './locale';
export type { MessageKey } from './messages/en';

export {
  LocaleLoadError,
  getI18nState,
  getLocale,
  initializeI18n,
  setLanguageMode,
  t,
  translateKey,
} from './runtime.svelte';

export type {
  I18nInitializationResult,
  I18nRuntimeState,
  I18nSwitchResult,
} from './runtime.svelte';
