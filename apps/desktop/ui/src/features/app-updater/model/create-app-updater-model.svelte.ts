import { check } from '@tauri-apps/plugin-updater';
import { ask } from '@tauri-apps/plugin-dialog';
import { relaunch } from '@tauri-apps/plugin-process';
import { getVersion } from '@tauri-apps/api/app';
import { toast } from 'svelte-sonner';
import { t } from '@shared/i18n';

export type AppUpdaterModel = ReturnType<typeof createAppUpdaterModel>;

export function createAppUpdaterModel() {
  let isCheckingForUpdates = $state(false);
  let isDownloading = $state(false);
  let appVersion = $state<string | null>(null);

  async function init(): Promise<void> {
    try {
      appVersion = await getVersion();
    } catch (e) {
      console.warn('Failed to get app version', e);
    }
  }

  async function handleCheckForUpdates(): Promise<void> {
    if (isCheckingForUpdates || isDownloading) {
      return;
    }

    try {
      isCheckingForUpdates = true;
      const update = await check();

      if (update) {
        const shouldInstall = await ask(
          t('settings.about.updateAvailable', { version: update.version }),
          {
            title: t('settings.about.title'),
            kind: 'info',
          },
        );

        if (shouldInstall) {
          isDownloading = true;
          try {
            await update.downloadAndInstall();
            await relaunch();
          } finally {
            isDownloading = false;
          }
        }
      } else {
        toast.success(t('settings.about.upToDate'));
      }
    } catch (e) {
      console.error('Failed to check for updates:', e);
      toast.error(t('settings.about.updateError'));
    } finally {
      isCheckingForUpdates = false;
    }
  }

  return {
    get isCheckingForUpdates() {
      return isCheckingForUpdates;
    },
    get isDownloading() {
      return isDownloading;
    },
    get appVersion() {
      return appVersion;
    },
    init,
    handleCheckForUpdates,
  };
}
