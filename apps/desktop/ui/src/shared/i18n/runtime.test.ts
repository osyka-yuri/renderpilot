import { describe, expect, it, vi } from 'vitest';

import type { LanguageMode, Locale } from './locale';
import { createI18nRuntime } from './runtime.svelte';
import type { LocalePack } from './packs/types';

type TestMessage = {
  nav: string;
  dynamic?: string;
};

function pack(
  locale: Locale,
  messages: TestMessage,
  dynamicCatalogs: LocalePack['dynamicCatalogs'] = [],
): LocalePack {
  return {
    locale,
    messages: {
      'nav.games': messages.nav,
      ...(messages.dynamic ? { 'dynamic.key': messages.dynamic } : {}),
    } as LocalePack['messages'],
    dynamicCatalogs,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });

  return { promise, resolve, reject };
}

function createTestRuntime(options: {
  storedMode?: LanguageMode;
  loaders?: Partial<Record<Locale, () => Promise<LocalePack>>>;
  systemLocale?: Locale;
}) {
  const baseEnglishPack = pack('en', { nav: 'Games' });
  const enPack: LocalePack = {
    ...baseEnglishPack,
    messages: {
      ...baseEnglishPack.messages,
      'english.only': 'English fallback',
    } as LocalePack['messages'],
  };
  const storedMode = options.storedMode ?? 'en';
  const systemLocale = options.systemLocale ?? 'en';
  let persistedMode = storedMode;
  let documentLocale: Locale = 'en';
  const persistedModes: LanguageMode[] = [];
  const documentLocales: Locale[] = [];
  const fallbackLoader = () => Promise.resolve(enPack);
  const loaders = {
    en: fallbackLoader,
    ru: fallbackLoader,
    es: fallbackLoader,
    zh: fallbackLoader,
    fr: fallbackLoader,
    de: fallbackLoader,
    ja: fallbackLoader,
    ...options.loaders,
  } satisfies Record<Locale, () => Promise<LocalePack>>;

  const runtime = createI18nRuntime({
    fallbackPack: enPack,
    loaders,
    readStoredMode: () => storedMode,
    persistMode: (mode) => {
      persistedMode = mode;
      persistedModes.push(mode);
    },
    resolveMode: (mode) => (mode === 'system' ? systemLocale : mode),
    applyDocumentLocale: (locale) => {
      documentLocale = locale;
      documentLocales.push(locale);
    },
  });

  return {
    runtime,
    get persistedMode() {
      return persistedMode;
    },
    get documentLocale() {
      return documentLocale;
    },
    persistedModes,
    documentLocales,
  };
}

describe('createI18nRuntime', () => {
  it('initializes the saved locale before exposing a ready state', async () => {
    const ru = deferred<LocalePack>();
    const test = createTestRuntime({
      storedMode: 'ru',
      loaders: {
        ru: () => ru.promise,
      },
    });

    const initialization = test.runtime.initializeI18n();
    expect(test.runtime.getState().status).toBe('loading');
    expect(test.runtime.translateKey('nav.games', 'fallback')).toBe('Games');

    ru.resolve(pack('ru', { nav: 'Игры' }));
    const result = await initialization;

    expect(result).toMatchObject({
      activeMode: 'ru',
      activeLocale: 'ru',
      fallbackUsed: false,
      error: null,
    });
    expect(test.runtime.getState().status).toBe('ready');
    expect(test.runtime.translateKey('nav.games', 'fallback')).toBe('Игры');
    expect(test.documentLocale).toBe('ru');
  });

  it('keeps the active pack and applies only the latest request', async () => {
    const ru = deferred<LocalePack>();
    const zh = deferred<LocalePack>();
    const ruLoader = vi.fn(() => ru.promise);
    const test = createTestRuntime({
      loaders: {
        ru: ruLoader,
        zh: () => zh.promise,
      },
    });

    const ruRequest = test.runtime.setLanguageMode('ru');
    expect(test.runtime.getState().status).toBe('loading');
    expect(test.runtime.translateKey('nav.games', 'fallback')).toBe('Games');

    const zhRequest = test.runtime.setLanguageMode('zh');
    zh.resolve(pack('zh', { nav: '游戏' }));
    expect(await zhRequest).toEqual({ outcome: 'applied', mode: 'zh', locale: 'zh' });

    ru.resolve(pack('ru', { nav: 'Игры' }));
    expect(await ruRequest).toEqual({ outcome: 'superseded', mode: 'ru', locale: 'ru' });
    expect(test.runtime.getLocale()).toBe('zh');
    expect(test.runtime.translateKey('nav.games', 'fallback')).toBe('游戏');
    expect(test.persistedModes).toEqual(['zh']);

    await test.runtime.setLanguageMode('ru');
    expect(ruLoader).toHaveBeenCalledTimes(1);
    expect(test.runtime.translateKey('nav.games', 'fallback')).toBe('Игры');
  });

  it('deduplicates same-locale imports and caches successful packs', async () => {
    const ru = deferred<LocalePack>();
    const loader = vi.fn(() => ru.promise);
    const test = createTestRuntime({ loaders: { ru: loader } });

    const firstRequest = test.runtime.setLanguageMode('ru');
    const secondRequest = test.runtime.setLanguageMode('ru');
    expect(secondRequest).toBe(firstRequest);

    ru.resolve(pack('ru', { nav: 'Игры' }));
    await Promise.all([firstRequest, secondRequest]);
    await test.runtime.setLanguageMode('en');
    await test.runtime.setLanguageMode('ru');

    expect(loader).toHaveBeenCalledTimes(1);
  });

  it('retains the previous pack on a winning failure and permits another attempt', async () => {
    const ru = pack('ru', { nav: 'Игры' });
    const loader = vi
      .fn<() => Promise<LocalePack>>()
      .mockRejectedValueOnce(new Error('missing chunk'))
      .mockResolvedValueOnce(ru);
    const test = createTestRuntime({ loaders: { ru: loader } });

    await expect(test.runtime.setLanguageMode('ru')).rejects.toMatchObject({
      code: 'i18n_locale_load_failed',
      locale: 'ru',
    });
    expect(test.runtime.getLocale()).toBe('en');
    expect(test.runtime.translateKey('nav.games', 'fallback')).toBe('Games');
    expect(test.runtime.getState().status).toBe('error');
    expect(test.persistedMode).toBe('en');
    expect(test.documentLocale).toBe('en');

    await expect(test.runtime.setLanguageMode('ru')).resolves.toEqual({
      outcome: 'applied',
      mode: 'ru',
      locale: 'ru',
    });
    expect(loader).toHaveBeenCalledTimes(2);
    expect(test.persistedMode).toBe('ru');
  });

  it('keeps the active locale when a later switch fails', async () => {
    const test = createTestRuntime({
      loaders: {
        ru: () => Promise.resolve(pack('ru', { nav: 'Игры' })),
        zh: () => Promise.reject(new Error('missing chunk')),
      },
    });

    await test.runtime.setLanguageMode('ru');
    await expect(test.runtime.setLanguageMode('zh')).rejects.toMatchObject({
      code: 'i18n_locale_load_failed',
      locale: 'zh',
    });

    expect(test.runtime.getState()).toMatchObject({
      status: 'error',
      activeMode: 'ru',
      activeLocale: 'ru',
      pending: null,
    });
    expect(test.runtime.getLocale()).toBe('ru');
    expect(test.runtime.translateKey('nav.games', 'fallback')).toBe('Игры');
    expect(test.persistedModes).toEqual(['ru']);
    expect(test.documentLocale).toBe('ru');
  });

  it('rejects malformed locale packs before activation', async () => {
    const test = createTestRuntime({
      loaders: {
        ru: () =>
          Promise.resolve({
            locale: 'ru',
            messages: {},
            dynamicCatalogs: null,
          } as unknown as LocalePack),
      },
    });

    await expect(test.runtime.setLanguageMode('ru')).rejects.toMatchObject({
      code: 'i18n_locale_load_failed',
      locale: 'ru',
    });
    expect(test.runtime.getLocale()).toBe('en');
    expect(test.persistedModes).toEqual([]);
  });

  it('supersedes a pending import immediately when the active pack is selected', async () => {
    const ru = deferred<LocalePack>();
    const test = createTestRuntime({
      loaders: {
        ru: () => ru.promise,
      },
    });

    const ruRequest = test.runtime.setLanguageMode('ru');
    const englishRequest = test.runtime.setLanguageMode('en');

    expect(test.runtime.getState()).toMatchObject({
      status: 'idle',
      activeMode: 'en',
      activeLocale: 'en',
      pending: null,
    });
    await expect(englishRequest).resolves.toEqual({
      outcome: 'applied',
      mode: 'en',
      locale: 'en',
    });

    ru.resolve(pack('ru', { nav: 'Игры' }));
    await expect(ruRequest).resolves.toEqual({
      outcome: 'superseded',
      mode: 'ru',
      locale: 'ru',
    });
    expect(test.runtime.getLocale()).toBe('en');
    expect(test.persistedModes).toEqual(['en']);
  });

  it('does not publish a superseded failure into runtime state', async () => {
    const ru = deferred<LocalePack>();
    const zh = deferred<LocalePack>();
    const test = createTestRuntime({
      loaders: {
        ru: () => ru.promise,
        zh: () => zh.promise,
      },
    });

    const ruRequest = test.runtime.setLanguageMode('ru');
    const zhRequest = test.runtime.setLanguageMode('zh');

    ru.reject(new Error('stale chunk failure'));
    await expect(ruRequest).resolves.toEqual({
      outcome: 'superseded',
      mode: 'ru',
      locale: 'ru',
    });
    expect(test.runtime.getState()).toMatchObject({
      status: 'loading',
      pending: { mode: 'zh', locale: 'zh' },
      error: null,
    });

    zh.resolve(pack('zh', { nav: '游戏' }));
    await expect(zhRequest).resolves.toEqual({
      outcome: 'applied',
      mode: 'zh',
      locale: 'zh',
    });
    expect(test.persistedModes).toEqual(['zh']);
  });

  it('shares one locale import across different modes while the last mode wins', async () => {
    const ru = deferred<LocalePack>();
    const loader = vi.fn(() => ru.promise);
    const test = createTestRuntime({
      systemLocale: 'ru',
      loaders: { ru: loader },
    });

    const systemRequest = test.runtime.setLanguageMode('system');
    const explicitRequest = test.runtime.setLanguageMode('ru');
    ru.resolve(pack('ru', { nav: 'Игры' }));

    await expect(systemRequest).resolves.toEqual({
      outcome: 'superseded',
      mode: 'system',
      locale: 'ru',
    });
    await expect(explicitRequest).resolves.toEqual({
      outcome: 'applied',
      mode: 'ru',
      locale: 'ru',
    });
    expect(loader).toHaveBeenCalledTimes(1);
    expect(test.runtime.getState()).toMatchObject({
      status: 'idle',
      activeMode: 'ru',
      activeLocale: 'ru',
    });
    expect(test.persistedModes).toEqual(['ru']);
  });

  it('preserves dynamic catalog precedence inside the active pack', async () => {
    const test = createTestRuntime({
      loaders: {
        ru: () =>
          Promise.resolve(
            pack('ru', { nav: 'Игры', dynamic: 'static wins' }, [
              { 'dynamic.key': 'dynamic fallback' },
              { 'other.key': 'dynamic value' },
            ]),
          ),
      },
    });

    await test.runtime.setLanguageMode('ru');
    expect(test.runtime.translateKey('dynamic.key', 'caller fallback')).toBe('static wins');
    expect(test.runtime.translateKey('other.key', 'caller fallback')).toBe('dynamic value');
    expect(test.runtime.translateKey('english.only', 'caller fallback')).toBe('English fallback');
    expect(test.runtime.translateKey('unknown.key', 'caller fallback')).toBe('caller fallback');
  });

  it('falls back to English when startup locale loading fails without overwriting storage', async () => {
    const test = createTestRuntime({
      storedMode: 'ru',
      loaders: {
        ru: () => Promise.reject(new Error('broken asset')),
      },
    });

    const result = await test.runtime.initializeI18n();

    expect(result.fallbackUsed).toBe(true);
    expect(result.activeMode).toBe('en');
    expect(result.activeLocale).toBe('en');
    expect(result.error?.locale).toBe('ru');
    expect(test.persistedMode).toBe('ru');
    expect(test.documentLocale).toBe('en');
  });

  it('keeps system mode while falling back to English during startup', async () => {
    const test = createTestRuntime({
      storedMode: 'system',
      systemLocale: 'ru',
      loaders: {
        ru: () => Promise.reject(new Error('broken asset')),
      },
    });

    const result = await test.runtime.initializeI18n();

    expect(result).toMatchObject({
      activeMode: 'system',
      activeLocale: 'en',
      fallbackUsed: true,
    });
    expect(test.runtime.getState()).toMatchObject({
      status: 'error',
      activeMode: 'system',
      activeLocale: 'en',
    });
    expect(test.persistedModes).toEqual([]);
  });

  it('initializes idempotently with one loader invocation and one commit', async () => {
    const ru = deferred<LocalePack>();
    const loader = vi.fn(() => ru.promise);
    const test = createTestRuntime({
      storedMode: 'ru',
      loaders: { ru: loader },
    });

    const first = test.runtime.initializeI18n();
    const second = test.runtime.initializeI18n();
    expect(second).toBe(first);

    ru.resolve(pack('ru', { nav: 'Игры' }));
    const [firstResult, secondResult] = await Promise.all([first, second]);

    expect(firstResult).toEqual(secondResult);
    expect(loader).toHaveBeenCalledTimes(1);
    expect(test.documentLocales).toEqual(['ru']);
    expect(test.persistedModes).toEqual([]);
  });
});
