import { afterEach, describe, expect, it, vi } from 'vitest';

import { encodeStoredLanguageMode } from './language-mode-codec';
import {
  LANGUAGE_MODE_STORAGE_KEY,
  persistLanguageMode,
  readStoredLanguageMode,
} from './language-mode-storage';
import { createStorage, installWindow } from './locale.test-support';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('language mode storage', () => {
  it.each([
    ['system', 'system'],
    ['en', 'en'],
    ['ru', 'ru'],
    ['es', 'es'],
    ['fr', 'fr'],
    ['de', 'de'],
    ['ja', 'ja'],
    ['zh', 'zh-Hans'],
  ] as const)('migrates legacy %s to %s', (legacy, expected) => {
    const storage = createStorage(legacy);
    installWindow(storage);

    expect(readStoredLanguageMode()).toBe(expected);
    expect(storage.getItem(LANGUAGE_MODE_STORAGE_KEY)).toBe(encodeStoredLanguageMode(expected));
  });

  it.each([null, '', '{', '{"version":1,"mode":"ru"}', '{"version":2,"mode":"zh"}'])(
    'does not rewrite corrupt or unsupported input: %j',
    (raw) => {
      const storage = createStorage(raw);
      installWindow(storage);

      expect(readStoredLanguageMode()).toBe('system');
      expect(storage.getItem(LANGUAGE_MODE_STORAGE_KEY)).toBe(raw);
    },
  );

  it('keeps the migrated mode when the migration write fails', () => {
    const storage = createStorage('zh');
    storage.setItem = () => {
      throw new Error('quota exceeded');
    };
    installWindow(storage);

    expect(readStoredLanguageMode()).toBe('zh-Hans');
  });

  it('writes v2 and ignores storage access failures', () => {
    const storage = createStorage();
    installWindow(storage);
    persistLanguageMode('zh-Hant');
    expect(storage.getItem(LANGUAGE_MODE_STORAGE_KEY)).toBe('{"version":2,"mode":"zh-Hant"}');

    vi.stubGlobal('window', {
      get localStorage() {
        throw new Error('disabled');
      },
    });
    expect(() => {
      persistLanguageMode('ru');
    }).not.toThrow();
    expect(readStoredLanguageMode()).toBe('system');
  });
});
