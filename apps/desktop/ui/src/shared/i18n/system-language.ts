import { negotiateLocale } from './locale-negotiation';
import { type LanguageMode, type Locale } from './locale-model';

/** Resolves a user preference into the concrete pack locale to render. */
export function resolveLocale(mode: LanguageMode): Locale {
  return mode === 'system' ? negotiateLocale(readNavigatorLanguageTags()) : mode;
}

export function observeSystemLanguage(listener: () => void): () => void {
  if (typeof window === 'undefined' || typeof window.addEventListener !== 'function') {
    return noop;
  }

  try {
    window.addEventListener('languagechange', listener);
  } catch {
    return noop;
  }

  return () => {
    try {
      window.removeEventListener('languagechange', listener);
    } catch {
      // Listener cleanup is best-effort during webview shutdown.
    }
  };
}

function readNavigatorLanguageTags(): readonly string[] {
  if (typeof navigator === 'undefined') {
    return [];
  }

  try {
    const languages = Array.from(navigator.languages);
    if (languages.length > 0) {
      return languages;
    }
  } catch {
    // Fall back to navigator.language below.
  }

  try {
    return navigator.language === '' ? [] : [navigator.language];
  } catch {
    return [];
  }
}

function noop(): void {
  return;
}
