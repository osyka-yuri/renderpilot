import '@shared/theme';
import { mount } from 'svelte';

import DesktopApp from '@app/routes/DesktopApp.svelte';
import { isDesktopPreviewMode } from '@shared/api-preview';
import { registerMockInvoker } from '@app/mocks/desktop';
import { getAppInitializationState, type AppInitializationState } from '@entities/app';

if (isDesktopPreviewMode()) {
  registerMockInvoker();
}

const target = document.getElementById('app');

if (!target) {
  throw new Error("Render root '#app' was not found.");
}

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
    return {
      isElevated: true,
      elevationSupported: false,
      elevationUserDeclined: false,
      elevationAttempted: false,
    };
  }
}

const initState = await loadInitialization();

const app = mount(DesktopApp, {
  target,
  props: { initState },
});

export default app;
