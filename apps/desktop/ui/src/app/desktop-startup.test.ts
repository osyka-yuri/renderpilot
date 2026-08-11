import { describe, expect, it, vi } from 'vitest';

import type { I18nInitializationResult } from '@shared/i18n';
import { loadDesktopStartup } from './desktop-startup';

const englishInitialization: I18nInitializationResult = {
  activeMode: 'en',
  activeLocale: 'en',
  fallbackUsed: false,
  error: null,
};

describe('loadDesktopStartup', () => {
  it('applies theme and prepares preview before starting pre-mount work in parallel', async () => {
    const preview = Promise.withResolvers<undefined>();
    const i18n = Promise.withResolvers<I18nInitializationResult>();
    const desktopApp = Promise.withResolvers<{ default: string }>();
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
    });
    const mount = vi.fn();
    void startup.then(mount);

    expect(events).toEqual(['theme', 'preview']);
    expect(mount).not.toHaveBeenCalled();

    preview.resolve(undefined);
    await vi.waitFor(() => {
      expect(events).toEqual(['theme', 'preview', 'i18n', 'desktop']);
    });

    i18n.resolve(englishInitialization);
    desktopApp.resolve({ default: 'DesktopApp' });
    await Promise.resolve();
    expect(mount).not.toHaveBeenCalled();

    await expect(startup).resolves.toEqual({
      i18n: englishInitialization,
      desktopAppModule: { default: 'DesktopApp' },
    });
    expect(mount).toHaveBeenCalledTimes(1);
  });
});
