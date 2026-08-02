import { decodeStoredLanguageMode, encodeStoredLanguageMode } from './language-mode-codec';
import { DEFAULT_LANGUAGE_MODE, type LanguageMode } from './locale-model';

export const LANGUAGE_MODE_STORAGE_KEY = 'renderpilot.language-mode';

export function readStoredLanguageMode(): LanguageMode {
  const storage = getLocalStorage();
  if (storage === null) {
    return DEFAULT_LANGUAGE_MODE;
  }

  try {
    const decoded = decodeStoredLanguageMode(storage.getItem(LANGUAGE_MODE_STORAGE_KEY));
    if (decoded.migrate) {
      writeStoredLanguageMode(storage, decoded.mode);
    }
    return decoded.mode;
  } catch {
    return DEFAULT_LANGUAGE_MODE;
  }
}

export function persistLanguageMode(mode: LanguageMode): void {
  const storage = getLocalStorage();
  if (storage !== null) {
    writeStoredLanguageMode(storage, mode);
  }
}

function writeStoredLanguageMode(storage: Storage, mode: LanguageMode): void {
  try {
    storage.setItem(LANGUAGE_MODE_STORAGE_KEY, encodeStoredLanguageMode(mode));
  } catch {
    // Ignore private mode, quota, permissions, and shutdown failures.
  }
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
