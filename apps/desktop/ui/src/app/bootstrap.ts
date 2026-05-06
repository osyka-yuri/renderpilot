import '@shared/theme/global.css';
import { mount } from 'svelte';

import DesktopApp from '@app/routes/DesktopApp.svelte';

const target = document.getElementById('app');

if (!target) {
  throw new Error("Render root '#app' was not found.");
}

const app = mount(DesktopApp, {
  target,
});

export default app;