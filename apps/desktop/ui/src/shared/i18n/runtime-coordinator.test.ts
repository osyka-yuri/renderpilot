import { describe, expect, it, vi } from 'vitest';

import type { LocalePack } from './packs/types';
import { createTestRuntime, deferred, pack } from './runtime.test-support';

describe('runtime coordinator integration', () => {
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
    expect(test.runtime.translateExternalMessage({ key: 'nav.games', fallback: 'fallback' })).toBe(
      'Games',
    );

    ru.resolve(pack('ru', { nav: 'Игры' }));
    const result = await initialization;

    expect(result).toMatchObject({
      activeMode: 'ru',
      activeLocale: 'ru',
      fallbackUsed: false,
      error: null,
    });
    expect(test.runtime.getState().status).toBe('ready');
    expect(test.runtime.translateExternalMessage({ key: 'nav.games', fallback: 'fallback' })).toBe(
      'Игры',
    );
    expect(test.documentLocale).toBe('ru');
  });

  it('keeps the active pack and applies only the latest request', async () => {
    const ru = deferred<LocalePack>();
    const zhHant = deferred<LocalePack>();
    const ruLoader = vi.fn(() => ru.promise);
    const test = createTestRuntime({
      loaders: {
        ru: ruLoader,
        'zh-Hant': () => zhHant.promise,
      },
    });

    const ruRequest = test.runtime.setLanguageMode('ru');
    expect(test.runtime.getState().status).toBe('loading');
    expect(test.runtime.translateExternalMessage({ key: 'nav.games', fallback: 'fallback' })).toBe(
      'Games',
    );

    const zhHantRequest = test.runtime.setLanguageMode('zh-Hant');
    zhHant.resolve(pack('zh-Hant', { nav: '遊戲' }));
    expect(await zhHantRequest).toEqual({
      outcome: 'applied',
      mode: 'zh-Hant',
      locale: 'zh-Hant',
    });

    ru.resolve(pack('ru', { nav: 'Игры' }));
    expect(await ruRequest).toEqual({ outcome: 'superseded', mode: 'ru', locale: 'ru' });
    expect(test.runtime.getLocale()).toBe('zh-Hant');
    expect(test.runtime.translateExternalMessage({ key: 'nav.games', fallback: 'fallback' })).toBe(
      '遊戲',
    );
    expect(test.persistedModes).toEqual(['zh-Hant']);

    await test.runtime.setLanguageMode('ru');
    expect(ruLoader).toHaveBeenCalledTimes(1);
    expect(test.runtime.translateExternalMessage({ key: 'nav.games', fallback: 'fallback' })).toBe(
      'Игры',
    );
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
    expect(test.runtime.translateExternalMessage({ key: 'nav.games', fallback: 'fallback' })).toBe(
      'Games',
    );
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

  it('allows an immediate retry from the user transition rejection handler', async () => {
    const loader = vi
      .fn<() => Promise<LocalePack>>()
      .mockRejectedValueOnce(new Error('missing chunk'))
      .mockResolvedValueOnce(pack('ru', { nav: 'Игры' }));
    const test = createTestRuntime({ loaders: { ru: loader } });

    const retry = test.runtime
      .setLanguageMode('ru')
      .catch(() => test.runtime.setLanguageMode('ru'));

    await expect(retry).resolves.toEqual({ outcome: 'applied', mode: 'ru', locale: 'ru' });
    expect(loader).toHaveBeenCalledTimes(2);
    expect(test.runtime.getLocale()).toBe('ru');
    expect(test.persistedModes).toEqual(['ru']);
  });

  it('keeps the active locale when a later switch fails', async () => {
    const test = createTestRuntime({
      loaders: {
        ru: () => Promise.resolve(pack('ru', { nav: 'Игры' })),
        'zh-Hant': () => Promise.reject(new Error('missing chunk')),
      },
    });

    await test.runtime.setLanguageMode('ru');
    await expect(test.runtime.setLanguageMode('zh-Hant')).rejects.toMatchObject({
      code: 'i18n_locale_load_failed',
      locale: 'zh-Hant',
    });

    expect(test.runtime.getState()).toMatchObject({
      status: 'error',
      activeMode: 'ru',
      activeLocale: 'ru',
      pending: null,
    });
    expect(test.runtime.getLocale()).toBe('ru');
    expect(test.runtime.translateExternalMessage({ key: 'nav.games', fallback: 'fallback' })).toBe(
      'Игры',
    );
    expect(test.persistedModes).toEqual(['ru']);
    expect(test.documentLocale).toBe('ru');
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
    const zhHant = deferred<LocalePack>();
    const test = createTestRuntime({
      loaders: {
        ru: () => ru.promise,
        'zh-Hant': () => zhHant.promise,
      },
    });

    const ruRequest = test.runtime.setLanguageMode('ru');
    const zhHantRequest = test.runtime.setLanguageMode('zh-Hant');

    ru.reject(new Error('stale chunk failure'));
    await expect(ruRequest).resolves.toEqual({
      outcome: 'superseded',
      mode: 'ru',
      locale: 'ru',
    });
    expect(test.runtime.getState()).toMatchObject({
      status: 'loading',
      pending: { mode: 'zh-Hant', locale: 'zh-Hant' },
      error: null,
    });

    zhHant.resolve(pack('zh-Hant', { nav: '遊戲' }));
    await expect(zhHantRequest).resolves.toEqual({
      outcome: 'applied',
      mode: 'zh-Hant',
      locale: 'zh-Hant',
    });
    expect(test.persistedModes).toEqual(['zh-Hant']);
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
    expect(
      test.runtime.translateExternalMessage({
        key: 'dynamic.key',
        fallback: 'caller fallback',
      }),
    ).toBe('static wins');
    expect(
      test.runtime.translateExternalMessage({ key: 'other.key', fallback: 'caller fallback' }),
    ).toBe('dynamic value');
    expect(
      test.runtime.translateExternalMessage({
        key: 'english.only',
        fallback: 'caller fallback',
      }),
    ).toBe('English fallback');
    expect(
      test.runtime.translateExternalMessage({ key: 'unknown.key', fallback: 'caller fallback' }),
    ).toBe('caller fallback');
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

  it('falls back coherently when startup document activation fails', async () => {
    const test = createTestRuntime({
      storedMode: 'ru',
      loaders: {
        ru: () => Promise.resolve(pack('ru', { nav: 'Игры' })),
      },
      applyDocumentLocale: (locale) => {
        if (locale === 'ru') {
          throw new Error('document is unavailable');
        }
      },
    });

    await expect(test.runtime.initializeI18n()).resolves.toMatchObject({
      activeMode: 'en',
      activeLocale: 'en',
      fallbackUsed: true,
      error: {
        code: 'i18n_locale_load_failed',
        mode: 'ru',
        locale: 'ru',
      },
    });
    expect(test.runtime.getState()).toMatchObject({
      status: 'error',
      activeMode: 'en',
      activeLocale: 'en',
    });
    expect(test.documentLocale).toBe('en');
    expect(test.runtime.translateExternalMessage({ key: 'nav.games', fallback: 'fallback' })).toBe(
      'Games',
    );
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

  it('observes live system language changes atomically without rewriting the preference', async () => {
    const ru = deferred<LocalePack>();
    const test = createTestRuntime({
      storedMode: 'system',
      loaders: { ru: () => ru.promise },
    });
    await test.runtime.initializeI18n();

    expect(test.systemObserverCount).toBe(1);
    test.setSystemLocale('ru');
    expect(test.runtime.getState()).toMatchObject({
      status: 'loading',
      activeMode: 'system',
      activeLocale: 'en',
      pending: { mode: 'system', locale: 'ru' },
    });
    expect(test.runtime.translateExternalMessage({ key: 'nav.games', fallback: 'fallback' })).toBe(
      'Games',
    );

    ru.resolve(pack('ru', { nav: 'Игры' }));
    await vi.waitFor(() => {
      expect(test.runtime.getLocale()).toBe('ru');
    });

    expect(test.runtime.getState()).toMatchObject({
      status: 'ready',
      activeMode: 'system',
      pending: null,
    });
    expect(test.persistedModes).toEqual([]);
    expect(test.documentLocale).toBe('ru');
  });

  it('derives the system observer lifecycle from the pending and active modes', async () => {
    const test = createTestRuntime({ storedMode: 'system' });
    await test.runtime.initializeI18n();
    expect(test.systemObserverCount).toBe(1);

    await test.runtime.setLanguageMode('ru');
    expect(test.systemObserverCount).toBe(0);

    const failure = new Error('broken locale');
    const failing = createTestRuntime({
      storedMode: 'system',
      loaders: { ru: () => Promise.reject(failure) },
    });
    await failing.runtime.initializeI18n();
    const transition = failing.runtime.setLanguageMode('ru');
    expect(failing.systemObserverCount).toBe(1);
    await expect(transition).rejects.toMatchObject({ locale: 'ru' });
    expect(failing.systemObserverCount).toBe(1);
  });

  it('reconciles the current system locale after an explicit switch fails', async () => {
    const ru = deferred<LocalePack>();
    const fr = deferred<LocalePack>();
    const test = createTestRuntime({
      storedMode: 'system',
      loaders: {
        ru: () => ru.promise,
        fr: () => fr.promise,
      },
    });
    await test.runtime.initializeI18n();

    const explicitRequest = test.runtime.setLanguageMode('ru');
    expect(test.systemObserverCount).toBe(1);
    test.setSystemLocale('fr');
    expect(test.runtime.getState().pending).toEqual({ mode: 'ru', locale: 'ru' });

    ru.reject(new Error('broken Russian pack'));
    await expect(explicitRequest).rejects.toMatchObject({ mode: 'ru', locale: 'ru' });
    await vi.waitFor(() => {
      expect(test.runtime.getState().pending).toEqual({ mode: 'system', locale: 'fr' });
    });

    fr.resolve(pack('fr', { nav: 'Jeux' }));
    await vi.waitFor(() => {
      expect(test.runtime.getLocale()).toBe('fr');
    });
    expect(test.runtime.getState()).toMatchObject({
      status: 'ready',
      activeMode: 'system',
      activeLocale: 'fr',
      pending: null,
    });
    expect(test.persistedModes).toEqual([]);
  });

  it('gives an immediate rejection-handler retry priority over system reconciliation', async () => {
    const firstRussianLoad = deferred<LocalePack>();
    const russianLoader = vi
      .fn<() => Promise<LocalePack>>()
      .mockImplementationOnce(() => firstRussianLoad.promise)
      .mockResolvedValueOnce(pack('ru', { nav: 'Игры' }));
    const frenchLoader = vi.fn(() => Promise.resolve(pack('fr', { nav: 'Jeux' })));
    const test = createTestRuntime({
      storedMode: 'system',
      loaders: {
        ru: russianLoader,
        fr: frenchLoader,
      },
    });
    await test.runtime.initializeI18n();

    const retry = test.runtime
      .setLanguageMode('ru')
      .catch(() => test.runtime.setLanguageMode('ru'));
    test.setSystemLocale('fr');
    firstRussianLoad.reject(new Error('transient Russian failure'));

    await expect(retry).resolves.toEqual({ outcome: 'applied', mode: 'ru', locale: 'ru' });
    await Promise.resolve();

    expect(russianLoader).toHaveBeenCalledTimes(2);
    expect(frenchLoader).not.toHaveBeenCalled();
    expect(test.runtime.getState()).toMatchObject({
      status: 'ready',
      activeMode: 'ru',
      activeLocale: 'ru',
      pending: null,
    });
    expect(test.persistedModes).toEqual(['ru']);
    expect(test.systemObserverCount).toBe(0);
  });

  it('retargets startup to the latest system locale before publishing readiness', async () => {
    const ru = deferred<LocalePack>();
    const fr = deferred<LocalePack>();
    const test = createTestRuntime({
      storedMode: 'system',
      systemLocale: 'ru',
      loaders: {
        ru: () => ru.promise,
        fr: () => fr.promise,
      },
    });

    const initialization = test.runtime.initializeI18n();
    test.setSystemLocale('fr');
    ru.resolve(pack('ru', { nav: 'Игры' }));
    await vi.waitFor(() => {
      expect(test.runtime.getState().pending).toEqual({ mode: 'system', locale: 'fr' });
    });

    fr.resolve(pack('fr', { nav: 'Jeux' }));
    await expect(initialization).resolves.toMatchObject({
      activeMode: 'system',
      activeLocale: 'fr',
      fallbackUsed: false,
    });
    expect(test.documentLocales).toEqual(['fr']);
    expect(test.persistedModes).toEqual([]);
  });

  it('carries the user persistence intent across a system-language retarget race', async () => {
    const fr = deferred<LocalePack>();
    const de = deferred<LocalePack>();
    const test = createTestRuntime({
      storedMode: 'en',
      systemLocale: 'fr',
      loaders: {
        fr: () => fr.promise,
        de: () => de.promise,
      },
    });
    await test.runtime.initializeI18n();

    const userRequest = test.runtime.setLanguageMode('system');
    expect(test.systemObserverCount).toBe(1);
    test.setSystemLocale('de');
    expect(test.runtime.getState().pending).toEqual({ mode: 'system', locale: 'de' });

    de.resolve(pack('de', { nav: 'Spiele' }));
    await vi.waitFor(() => {
      expect(test.runtime.getLocale()).toBe('de');
    });
    expect(test.persistedModes).toEqual(['system']);
    expect(test.runtime.getState().activeMode).toBe('system');

    fr.resolve(pack('fr', { nav: 'Jeux' }));
    await expect(userRequest).resolves.toEqual({
      outcome: 'applied',
      mode: 'system',
      locale: 'de',
    });
    expect(test.runtime.getLocale()).toBe('de');
    expect(test.persistedModes).toEqual(['system']);
  });

  it('reports the winning failure when a language event retargets a user system switch', async () => {
    const fr = deferred<LocalePack>();
    const de = deferred<LocalePack>();
    const test = createTestRuntime({
      storedMode: 'en',
      systemLocale: 'fr',
      loaders: {
        fr: () => fr.promise,
        de: () => de.promise,
      },
    });
    await test.runtime.initializeI18n();

    const userRequest = test.runtime.setLanguageMode('system');
    test.setSystemLocale('de');
    de.reject(new Error('broken German pack'));

    await expect(userRequest).rejects.toMatchObject({
      code: 'i18n_locale_load_failed',
      mode: 'system',
      locale: 'de',
    });
    expect(test.runtime.getState()).toMatchObject({
      status: 'error',
      activeMode: 'en',
      activeLocale: 'en',
      pending: null,
    });
    expect(test.persistedModes).toEqual([]);
    expect(test.systemObserverCount).toBe(0);

    fr.resolve(pack('fr', { nav: 'Jeux' }));
    await vi.waitFor(() => {
      expect(test.runtime.getLocale()).toBe('en');
    });
  });

  it('keeps pack, state, and document locale aligned when document activation fails', async () => {
    let rejectRussianDocumentLocale = true;
    const ruLoader = vi.fn(() => Promise.resolve(pack('ru', { nav: 'Игры' })));
    const test = createTestRuntime({
      loaders: { ru: ruLoader },
      applyDocumentLocale: (locale) => {
        if (locale === 'ru' && rejectRussianDocumentLocale) {
          throw new Error('document is unavailable');
        }
      },
    });
    await test.runtime.initializeI18n();

    await expect(test.runtime.setLanguageMode('ru')).rejects.toMatchObject({
      code: 'i18n_locale_load_failed',
      locale: 'ru',
    });
    expect(test.runtime.getLocale()).toBe('en');
    expect(test.documentLocale).toBe('en');
    expect(test.runtime.translateExternalMessage({ key: 'nav.games', fallback: 'fallback' })).toBe(
      'Games',
    );
    expect(test.persistedModes).toEqual([]);

    let cachedRequest!: Promise<unknown>;
    expect(() => {
      cachedRequest = test.runtime.setLanguageMode('ru');
    }).not.toThrow();
    await expect(cachedRequest).rejects.toMatchObject({ locale: 'ru' });
    expect(ruLoader).toHaveBeenCalledTimes(1);
    expect(test.runtime.getLocale()).toBe('en');

    rejectRussianDocumentLocale = false;
    await expect(test.runtime.setLanguageMode('ru')).resolves.toMatchObject({
      outcome: 'applied',
      locale: 'ru',
    });
    expect(test.runtime.getLocale()).toBe('ru');
    expect(test.documentLocale).toBe('ru');
    expect(test.persistedModes).toEqual(['ru']);
  });

  it('keeps system mode and permits retry after a background refresh failure', async () => {
    const loader = vi
      .fn<() => Promise<LocalePack>>()
      .mockRejectedValueOnce(new Error('broken asset'))
      .mockResolvedValueOnce(pack('ru', { nav: 'Игры' }));
    const test = createTestRuntime({
      storedMode: 'system',
      loaders: { ru: loader },
    });
    await test.runtime.initializeI18n();

    test.setSystemLocale('ru');
    await vi.waitFor(() => {
      expect(test.runtime.getState().status).toBe('error');
    });
    expect(test.runtime.getState()).toMatchObject({
      activeMode: 'system',
      activeLocale: 'en',
      pending: null,
    });
    expect(test.systemObserverCount).toBe(1);
    expect(test.persistedModes).toEqual([]);

    test.setSystemLocale('ru');
    await vi.waitFor(() => {
      expect(test.runtime.getLocale()).toBe('ru');
    });
    expect(loader).toHaveBeenCalledTimes(2);
    expect(test.runtime.getState().status).toBe('ready');
    expect(test.persistedModes).toEqual([]);
  });
});
