import { afterEach, describe, expect, it, vi } from 'vitest';

import { observeSystemLanguage, resolveLocale } from './system-language';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('system locale browser adapter', () => {
  it('uses navigator.languages and falls back to navigator.language only when needed', () => {
    vi.stubGlobal('navigator', { languages: ['de-DE', 'ru-RU'], language: 'fr-FR' });
    expect(resolveLocale('system')).toBe('de');

    vi.stubGlobal('navigator', { languages: [], language: 'fr-FR' });
    expect(resolveLocale('system')).toBe('fr');
    expect(resolveLocale('zh-Hant')).toBe('zh-Hant');
  });

  it('degrades to English when browser language access is unavailable or throws', () => {
    vi.stubGlobal('navigator', {
      get languages() {
        throw new Error('blocked');
      },
      get language() {
        throw new Error('blocked');
      },
    });
    expect(resolveLocale('system')).toBe('en');

    vi.stubGlobal('navigator', undefined);
    expect(resolveLocale('system')).toBe('en');
  });

  it('subscribes and removes the exact languagechange listener', () => {
    const addEventListener = vi.fn();
    const removeEventListener = vi.fn();
    vi.stubGlobal('window', { addEventListener, removeEventListener });
    const listener = vi.fn();

    const unsubscribe = observeSystemLanguage(listener);
    expect(addEventListener).toHaveBeenCalledWith('languagechange', listener);

    unsubscribe();
    expect(removeEventListener).toHaveBeenCalledWith('languagechange', listener);
  });

  it('returns a safe noop when subscription is unavailable or fails', () => {
    vi.stubGlobal('window', undefined);
    expect(() => {
      observeSystemLanguage(vi.fn())();
    }).not.toThrow();

    vi.stubGlobal('window', {
      addEventListener: () => {
        throw new Error('shutting down');
      },
    });
    expect(() => {
      observeSystemLanguage(vi.fn())();
    }).not.toThrow();
  });
});
