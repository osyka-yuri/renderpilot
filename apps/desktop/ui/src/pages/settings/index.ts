export async function loadSettingsPage() {
  return (await import('./ui/SettingsPage.svelte')).default;
}

export { settingsTabMemory } from './model/settings-page-model';
