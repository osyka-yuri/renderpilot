/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { I18nInitializationResult } from '@shared/i18n';

const mocks = vi.hoisted(() => ({
  mount: vi.fn(),
  applyThemeMode: vi.fn(),
  readStoredThemeMode: vi.fn(() => 'dark'),
  isDesktopPreviewMode: vi.fn(() => false),
  initializeI18n: vi.fn(),
  publishCommandErrorNotification: vi.fn(),
  loadDesktopStartup: vi.fn(),
}));

vi.mock('svelte', () => ({ mount: mocks.mount }));
vi.mock('@shared/theme', () => ({
  applyThemeMode: mocks.applyThemeMode,
  readStoredThemeMode: mocks.readStoredThemeMode,
}));
vi.mock('@shared/api-preview', () => ({
  isDesktopPreviewMode: mocks.isDesktopPreviewMode,
}));
vi.mock('@shared/i18n', () => ({
  initializeI18n: mocks.initializeI18n,
}));
vi.mock('@shared/notifications', () => ({
  publishCommandErrorNotification: mocks.publishCommandErrorNotification,
}));
vi.mock('./desktop-startup', () => ({
  loadDesktopStartup: mocks.loadDesktopStartup,
}));
vi.mock('@app/routes/DesktopApp.svelte', () => ({
  default: 'DesktopApp',
}));

function createLocaleLoadError(): NonNullable<I18nInitializationResult['error']> {
  const cause = new Error('chunk missing');

  return Object.assign(new Error('Failed to load locale pack "ru".'), {
    name: 'LocaleLoadError',
    code: 'i18n_locale_load_failed' as const,
    mode: 'ru' as const,
    locale: 'ru' as const,
    cause,
  });
}

describe('bootstrap', () => {
  beforeEach(() => {
    document.body.innerHTML = `
      <div
        id="app"
        data-startup-skeleton
        aria-busy="true"
        role="progressbar"
        aria-label="RenderPilot"
      >
      </div>
    `;
    mocks.mount.mockReset().mockReturnValue({ mounted: true });
    mocks.applyThemeMode.mockReset();
    mocks.readStoredThemeMode.mockReset().mockReturnValue('dark');
    mocks.isDesktopPreviewMode.mockReset().mockReturnValue(false);
    mocks.initializeI18n.mockReset();
    mocks.publishCommandErrorNotification.mockReset();
    mocks.loadDesktopStartup.mockReset();
  });

  afterEach(() => {
    document.body.replaceChildren();
    vi.resetModules();
  });

  it('keeps the skeleton until startup resolves, then mounts and publishes one locale error', async () => {
    const startup = Promise.withResolvers<{
      i18n: I18nInitializationResult;
      desktopAppModule: { default: string };
    }>();
    const localeError = createLocaleLoadError();

    mocks.loadDesktopStartup.mockReturnValue(startup.promise);
    const importBootstrap = import('./bootstrap');

    await vi.waitFor(() => {
      expect(mocks.loadDesktopStartup).toHaveBeenCalledOnce();
    });
    expect(document.querySelector('[data-startup-skeleton]')).not.toBeNull();
    expect(mocks.mount).not.toHaveBeenCalled();

    startup.resolve({
      i18n: {
        activeMode: 'en',
        activeLocale: 'en',
        fallbackUsed: true,
        error: localeError,
      },
      desktopAppModule: { default: 'DesktopApp' },
    });
    await importBootstrap;

    expect(document.querySelector('[data-startup-skeleton]')).toBeNull();
    expect(document.getElementById('app')?.getAttribute('aria-busy')).toBeNull();
    expect(document.getElementById('app')?.getAttribute('role')).toBeNull();
    expect(document.getElementById('app')?.getAttribute('aria-label')).toBeNull();
    expect(mocks.mount).toHaveBeenCalledOnce();
    expect(mocks.mount).toHaveBeenCalledWith('DesktopApp', {
      target: document.getElementById('app'),
    });
    expect(mocks.publishCommandErrorNotification).toHaveBeenCalledOnce();
    expect(mocks.publishCommandErrorNotification).toHaveBeenCalledWith(localeError);
  });
});
