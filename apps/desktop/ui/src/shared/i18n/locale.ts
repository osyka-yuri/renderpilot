/**
 * Locale primitives for the interface localization system.
 *
 * Mirrors the persistence/resolution shape of `@shared/theme`: pure functions
 * over `localStorage`, no framework coupling, safe in non-browser contexts.
 *
 * `LanguageMode` is the user-selectable preference ('system' follows the OS),
 * while `Locale` is the concrete, resolved language the catalog is keyed by.
 */

export type Locale = 'en' | 'ru';
export type LanguageMode = 'system' | Locale;

const STORAGE_KEY = 'renderpilot.language-mode';

const DEFAULT_LANGUAGE_MODE: LanguageMode = 'system';
const DEFAULT_LOCALE: Locale = 'en';

const LANGUAGE_MODES = ['system', 'en', 'ru'] as const;

export function readStoredLanguageMode(): LanguageMode {
  const storage = getLocalStorage();

  if (!storage) {
    return DEFAULT_LANGUAGE_MODE;
  }

  try {
    return normalizeLanguageMode(storage.getItem(STORAGE_KEY));
  } catch {
    return DEFAULT_LANGUAGE_MODE;
  }
}

export function persistLanguageMode(mode: LanguageMode): void {
  const storage = getLocalStorage();

  if (!storage) {
    return;
  }

  try {
    storage.setItem(STORAGE_KEY, mode);
  } catch {
    // Ignore storage errors: private mode, quota exceeded, disabled storage, etc.
  }
}

/**
 * Resolves a preference into the concrete language to render. 'system' is
 * derived from the browser/OS UI language reported to the webview.
 */
export function resolveLocale(mode: LanguageMode): Locale {
  if (mode !== 'system') {
    return mode;
  }

  return detectSystemLocale();
}

function detectSystemLocale(): Locale {
  if (typeof navigator === 'undefined') {
    return DEFAULT_LOCALE;
  }

  const language = navigator.language.toLowerCase();

  return language.startsWith('ru') ? 'ru' : DEFAULT_LOCALE;
}

function getLocalStorage(): Storage | null {
  if (typeof window === 'undefined') {
    return null;
  }

  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function normalizeLanguageMode(value: string | null): LanguageMode {
  return isLanguageMode(value) ? value : DEFAULT_LANGUAGE_MODE;
}

function isLanguageMode(value: string | null): value is LanguageMode {
  return value !== null && LANGUAGE_MODES.includes(value as LanguageMode);
}
