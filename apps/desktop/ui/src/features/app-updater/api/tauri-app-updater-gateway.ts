import { getVersion } from '@tauri-apps/api/app';
import { Channel } from '@tauri-apps/api/core';
import { relaunch } from '@tauri-apps/plugin-process';

import { invokeDesktop } from '@shared/api';

import type {
  AppUpdateDownloadEvent,
  AppUpdateHandle,
  AppUpdateInstallOutcome,
  AppUpdaterGateway,
} from './app-updater-gateway';
import type {
  AppUpdateApplyDto,
  AppUpdateCheckDto,
  AppUpdateDownloadEventDto,
} from './tauri-app-updater-contract';

/**
 * Tauri transport boundary for the app-updater feature. Native updater resources
 * stay in Rust; this module exposes only serializable DTOs to the UI model.
 */
export function createTauriAppUpdaterGateway(): AppUpdaterGateway {
  return {
    getCurrentVersion: () => getVersion(),

    async checkForUpdate(): Promise<AppUpdateHandle | null> {
      const update = await invokeDesktop<AppUpdateCheckDto | null>('app_update_check');
      if (update === null) {
        return null;
      }

      return createTauriUpdateHandle(update);
    },

    relaunch: () => relaunch(),
  };
}

function createTauriUpdateHandle(update: AppUpdateCheckDto): AppUpdateHandle {
  const { metadata, sessionId } = update;
  let installed = false;
  let activeDownload: Promise<void> | null = null;
  let closePromise: Promise<void> | null = null;

  return {
    metadata,

    download(onEvent: (event: AppUpdateDownloadEvent) => void): Promise<void> {
      if (closePromise) {
        return Promise.reject(new Error('updater session is closing'));
      }
      if (activeDownload) {
        return Promise.reject(new Error('updater download is already active'));
      }
      const channel = new Channel<AppUpdateDownloadEventDto>((event) => {
        onEvent(event);
      });

      const request = invokeDesktop<unknown>('app_update_download', {
        sessionId,
        onEvent: channel,
      }).then(() => undefined);
      const tracked = request.finally(() => {
        if (activeDownload === tracked) {
          activeDownload = null;
        }
      });
      activeDownload = tracked;
      return tracked;
    },

    async install(): Promise<AppUpdateInstallOutcome> {
      const result = await invokeDesktop<AppUpdateApplyDto>('app_update_apply', {
        sessionId,
      });
      installed = true;
      return result;
    },

    close(): Promise<void> {
      // A successful native apply consumes the Rust-side session. Calling
      // close afterwards would be redundant and may race native cleanup.
      if (installed) {
        return Promise.resolve();
      }

      closePromise ??= (async () => {
        try {
          await activeDownload;
        } catch {
          // A failed download restores the Rust session to a closable state.
        }
        await invokeDesktop('app_update_close', { sessionId });
      })();
      return closePromise;
    },
  };
}
