import { vi } from 'vitest';

import { LANGUAGE_MODE_STORAGE_KEY } from './language-mode-storage';

export function createStorage(initialValue: string | null = null): Storage {
  const values = new Map<string, string>();
  if (initialValue !== null) {
    values.set(LANGUAGE_MODE_STORAGE_KEY, initialValue);
  }

  return {
    get length() {
      return values.size;
    },
    clear: () => {
      values.clear();
    },
    getItem: (key) => values.get(key) ?? null,
    key: (index) => Array.from(values.keys())[index] ?? null,
    removeItem: (key) => values.delete(key),
    setItem: (key, value) => values.set(key, value),
  };
}

export function installWindow(storage: Storage): void {
  vi.stubGlobal('window', {
    localStorage: storage,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  });
}
