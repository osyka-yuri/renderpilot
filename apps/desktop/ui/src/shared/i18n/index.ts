export type { Locale, LanguageMode } from './locale';
export type {
  MessageKey,
  MessageKeyForParams,
  MessageKeyWithoutParams,
  MessageParams,
  MessageRef,
  ParameterizedMessageKey,
} from './messages/en';

export {
  LocaleLoadError,
  createMessageRef,
  getI18nState,
  getLocale,
  initializeI18n,
  setLanguageMode,
  t,
  translateExternalMessage,
  translateMessageRef,
} from './runtime.svelte';

export type {
  ExternalMessageInput,
  I18nInitializationResult,
  I18nRuntimeState,
  I18nSwitchResult,
} from './runtime.svelte';
