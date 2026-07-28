import { mount } from 'svelte';

import { isDesktopPreviewMode } from '@shared/api-preview';
import { publishCommandErrorNotification } from '@shared/notifications';
import { applyThemeMode, readStoredThemeMode } from '@shared/theme';
import { initializeI18n } from '@shared/i18n';
import {
  DEFAULT_APP_INITIALIZATION,
  getAppInitializationState,
  type AppInitializationState,
} from '@entities/app';
import { loadDesktopStartup } from './desktop-startup';

function getAppRoot(): HTMLElement {
  const root = document.getElementById('app');

  if (!root) {
    throw new Error("Render root '#app' was not found.");
  }

  return root;
}
const appRoot = getAppRoot();

/**
 * Retrieves the process-wide initialization snapshot (e.g., elevation status)
 * prior to mounting the user interface. This data is considered session-stable
 * and is fetched only once. It is provided as a static property, allowing the
 * application model to expose it through standard getters without incurring
 * reactive lifecycle overhead.
 *
 * Should the IPC call fail (a highly improbable scenario given the synchronous
 * nature of the Rust backend command), the system automatically gracefully
 * degrades to a safe-default snapshot, ensuring the UI mounts successfully.
 */
async function loadInitialization(): Promise<AppInitializationState> {
  try {
    return await getAppInitializationState();
  } catch {
    return DEFAULT_APP_INITIALIZATION;
  }
}

async function preparePreview(): Promise<void> {
  if (isDesktopPreviewMode()) {
    const { registerMockInvoker } = await import('@app/mocks/desktop');
    registerMockInvoker();
  }
}

const {
  i18n: i18nResult,
  desktopAppModule,
  initialization: initState,
} = await loadDesktopStartup({
  // Global theme CSS is imported before module evaluation; apply the persisted
  // mode before any await so the static skeleton cannot flash the wrong theme.
  applyStoredTheme: () => {
    applyThemeMode(readStoredThemeMode());
  },
  preparePreview,
  initializeI18n,
  importDesktopApp: () => import('@app/routes/DesktopApp.svelte'),
  loadInitialization,
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
  props: { initState },
});

if (i18nResult.error !== null) {
  publishCommandErrorNotification(i18nResult.error);
}

export default app;
