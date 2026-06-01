export type { Locale, LanguageMode } from './locale';
export { readStoredLanguageMode } from './locale';

export type { MessageKey } from './messages';

export { t, translateKey, setLanguageMode, getLocale, initI18n } from './i18n.svelte';
