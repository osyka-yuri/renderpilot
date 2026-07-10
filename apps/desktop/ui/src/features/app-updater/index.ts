export { createAppUpdaterModel } from './model/create-app-updater-model.svelte';
export { createTauriAppUpdaterGateway } from './api/tauri-app-updater-gateway';
export { default as AppUpdateDialog } from './ui/AppUpdateDialog.svelte';

export type {
  AppUpdateDialogState,
  AppUpdateNotice,
  AppUpdaterModel,
  AppUpdateOffer,
  SettingsUpdateAction,
} from './model/types';
