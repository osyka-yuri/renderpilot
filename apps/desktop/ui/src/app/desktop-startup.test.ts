import { describe, expect, it, vi } from 'vitest';

import type { I18nInitializationResult } from '@shared/i18n';
import { loadDesktopStartup } from './desktop-startup';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });

  return { promise, resolve };
}

const englishInitialization: I18nInitializationResult = {
  activeMode: 'en',
  activeLocale: 'en',
  fallbackUsed: false,
  error: null,
};

describe('loadDesktopStartup', () => {
  it('applies theme and prepares preview before starting all pre-mount work in parallel', async () => {
    const preview = deferred<undefined>();
    const i18n = deferred<I18nInitializationResult>();
    const desktopApp = deferred<{ default: string }>();
    const initialization = deferred<{ isElevated: boolean }>();
    const events: string[] = [];

    const startup = loadDesktopStartup({
      applyStoredTheme: () => events.push('theme'),
      preparePreview: () => {
        events.push('preview');
        return preview.promise;
      },
      initializeI18n: () => {
        events.push('i18n');
        return i18n.promise;
      },
      importDesktopApp: () => {
        events.push('desktop');
        return desktopApp.promise;
      },
      loadInitialization: () => {
        events.push('backend');
        return initialization.promise;
      },
    });
    const mount = vi.fn();
    void startup.then(mount);

    expect(events).toEqual(['theme', 'preview']);
    expect(mount).not.toHaveBeenCalled();

    preview.resolve(undefined);
    await vi.waitFor(() => {
      expect(events).toEqual(['theme', 'preview', 'i18n', 'desktop', 'backend']);
    });

    i18n.resolve(englishInitialization);
    desktopApp.resolve({ default: 'DesktopApp' });
    await Promise.resolve();
    expect(mount).not.toHaveBeenCalled();

    initialization.resolve({ isElevated: true });
    await expect(startup).resolves.toEqual({
      i18n: englishInitialization,
      desktopAppModule: { default: 'DesktopApp' },
      initialization: { isElevated: true },
    });
    expect(mount).toHaveBeenCalledTimes(1);
  });
});
