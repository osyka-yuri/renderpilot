import { getVersion } from '@tauri-apps/api/app';
import { relaunch } from '@tauri-apps/plugin-process';
import { check, type DownloadEvent, type Update } from '@tauri-apps/plugin-updater';

import type {
  AppUpdateDownloadEvent,
  AppUpdateHandle,
  AppUpdateMetadata,
  AppUpdaterGateway,
} from './app-updater-gateway';

/**
 * Sole module allowed to import @tauri-apps/plugin-updater and plugin-process
 * for the app-updater feature.
 */
export function createTauriAppUpdaterGateway(): AppUpdaterGateway {
  return {
    getCurrentVersion: () => getVersion(),

    async checkForUpdate(): Promise<AppUpdateHandle | null> {
      const update = await check();
      if (!update) {
        return null;
      }

      return createTauriUpdateHandle(update);
    },

    relaunch: () => relaunch(),
  };
}

function createTauriUpdateHandle(update: Update): AppUpdateHandle {
  const metadata = toMetadata(update);
  let installed = false;
  let closePromise: Promise<void> | null = null;

  return {
    metadata,

    download(onEvent: (event: AppUpdateDownloadEvent) => void): Promise<void> {
      return update.download((event) => {
        onEvent(mapDownloadEvent(event));
      });
    },

    async install(): Promise<void> {
      await update.install();
      installed = true;
    },

    close(): Promise<void> {
      // update.install() releases the Rust-side resources itself. Calling close after it
      // would be redundant and may race the updater's own cleanup.
      if (installed) {
        return Promise.resolve();
      }

      closePromise ??= update.close();
      return closePromise;
    },
  };
}

function toMetadata(update: Update): AppUpdateMetadata {
  return {
    currentVersion: update.currentVersion,
    version: update.version,
    date: update.date ?? null,
    body: update.body ?? '',
  };
}

function mapDownloadEvent(event: DownloadEvent): AppUpdateDownloadEvent {
  switch (event.event) {
    case 'Started':
      return {
        type: 'started',
        // Domain sanitization lives in progress.ts
        contentLength: event.data.contentLength ?? null,
      };
    case 'Progress':
      return {
        type: 'progress',
        chunkLength: event.data.chunkLength,
      };
    case 'Finished':
      return { type: 'finished' };
  }
}
