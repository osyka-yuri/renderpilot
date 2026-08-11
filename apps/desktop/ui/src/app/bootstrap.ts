import { mount } from 'svelte';

import { isDesktopPreviewMode } from '@shared/api-preview';
import { publishCommandErrorNotification } from '@shared/notifications';
import { applyThemeMode, readStoredThemeMode } from '@shared/theme';
import { initializeI18n } from '@shared/i18n';
import { loadDesktopStartup } from './desktop-startup';

function getAppRoot(): HTMLElement {
  const root = document.getElementById('app');

  if (!root) {
    throw new Error("Render root '#app' was not found.");
  }

  return root;
}
const appRoot = getAppRoot();

async function preparePreview(): Promise<void> {
  if (isDesktopPreviewMode()) {
    const { registerMockInvoker } = await import('@app/mocks/desktop');
    registerMockInvoker();
  }
}

const { i18n: i18nResult, desktopAppModule } = await loadDesktopStartup({
  // Global theme CSS is imported before module evaluation; apply the persisted
  // mode before any await so the static skeleton cannot flash the wrong theme.
  applyStoredTheme: () => {
    applyThemeMode(readStoredThemeMode());
  },
  preparePreview,
  initializeI18n,
  importDesktopApp: () => import('@app/routes/DesktopApp.svelte'),
});
const { default: DesktopApp } = desktopAppModule;

function finishStartup(): void {
  appRoot.replaceChildren();
  appRoot.removeAttribute('data-startup-skeleton');
  appRoot.removeAttribute('aria-busy');
  appRoot.removeAttribute('role');
  appRoot.removeAttribute('aria-label');
}

finishStartup();
const app = mount(DesktopApp, {
  target: appRoot,
});

if (i18nResult.error !== null) {
  publishCommandErrorNotification(i18nResult.error);
}

export default app;
