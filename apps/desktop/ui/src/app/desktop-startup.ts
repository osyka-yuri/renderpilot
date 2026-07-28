import type { I18nInitializationResult } from '@shared/i18n';

export type DesktopStartupDependencies<TDesktopAppModule, TInitialization> = Readonly<{
  applyStoredTheme: () => void;
  preparePreview: () => Promise<void>;
  initializeI18n: () => Promise<I18nInitializationResult>;
  importDesktopApp: () => Promise<TDesktopAppModule>;
  loadInitialization: () => Promise<TInitialization>;
}>;

export type DesktopStartupResult<TDesktopAppModule, TInitialization> = Readonly<{
  i18n: I18nInitializationResult;
  desktopAppModule: TDesktopAppModule;
  initialization: TInitialization;
}>;

/**
 * Coordinates only the pre-mount phase. Keeping this boundary independent from
 * Svelte makes startup ordering and concurrency deterministic and testable.
 */
export async function loadDesktopStartup<TDesktopAppModule, TInitialization>(
  deps: DesktopStartupDependencies<TDesktopAppModule, TInitialization>,
): Promise<DesktopStartupResult<TDesktopAppModule, TInitialization>> {
  deps.applyStoredTheme();
  await deps.preparePreview();

  const [i18n, desktopAppModule, initialization] = await Promise.all([
    deps.initializeI18n(),
    deps.importDesktopApp(),
    deps.loadInitialization(),
  ]);

  return { i18n, desktopAppModule, initialization };
}
