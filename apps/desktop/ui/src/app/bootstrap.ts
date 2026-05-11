import '@shared/theme';
import { mount } from 'svelte';

import DesktopApp from '@app/routes/DesktopApp.svelte';
import { isDesktopPreviewMode } from '@shared/api-preview';
import { registerMockInvoker } from '@app/mocks/desktop';

if (isDesktopPreviewMode()) {
  registerMockInvoker();
}

const target = document.getElementById('app');

if (!target) {
  throw new Error("Render root '#app' was not found.");
}

const app = mount(DesktopApp, {
  target,
});

export default app;
