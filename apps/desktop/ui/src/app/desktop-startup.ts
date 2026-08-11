import type { I18nInitializationResult } from '@shared/i18n';

export type DesktopStartupDependencies<TDesktopAppModule> = Readonly<{
  applyStoredTheme: () => void;
  preparePreview: () => Promise<void>;
  initializeI18n: () => Promise<I18nInitializationResult>;
  importDesktopApp: () => Promise<TDesktopAppModule>;
}>;

export type DesktopStartupResult<TDesktopAppModule> = Readonly<{
  i18n: I18nInitializationResult;
  desktopAppModule: TDesktopAppModule;
}>;

/**
 * Coordinates only the pre-mount phase. Keeping this boundary independent from
 * Svelte makes startup ordering and concurrency deterministic and testable.
 */
export async function loadDesktopStartup<TDesktopAppModule>(
  deps: DesktopStartupDependencies<TDesktopAppModule>,
): Promise<DesktopStartupResult<TDesktopAppModule>> {
  deps.applyStoredTheme();
  await deps.preparePreview();

  const [i18n, desktopAppModule] = await Promise.all([
    deps.initializeI18n(),
    deps.importDesktopApp(),
  ]);

  return { i18n, desktopAppModule };
}
