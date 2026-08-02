/**
 * Public locale primitives and browser adapters for the interface localization
 * system. Internal modules import the narrow owner module directly.
 */

export {
  decodeStoredLanguageMode,
  encodeStoredLanguageMode,
  type DecodedStoredLanguageMode,
} from './language-mode-codec';
export { persistLanguageMode, readStoredLanguageMode } from './language-mode-storage';
export { negotiateLocale } from './locale-negotiation';
export {
  LAZY_LOCALES,
  LOCALES,
  type LanguageMode,
  type LazyLocale,
  type Locale,
} from './locale-model';
export { observeSystemLanguage, resolveLocale } from './system-language';
