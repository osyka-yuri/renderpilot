/**
 * Feature-facing updater contract.
 *
 * Keeps Tauri Update / DownloadEvent / Resource types behind the gateway so
 * the model and UI only deal with serializable, owned shapes.
 */

export type AppUpdateMetadata = {
  currentVersion: string;
  version: string;
  date: string | null;
  body: string;
};

export type AppUpdateDownloadEvent =
  | {
      type: 'started';
      contentLength: number | null;
    }
  | {
      type: 'progress';
      chunkLength: number;
    }
  | {
      type: 'finished';
    };

export type AppUpdateHandle = {
  readonly metadata: AppUpdateMetadata;

  download: (onEvent: (event: AppUpdateDownloadEvent) => void) => Promise<void>;

  /**
   * Installs the downloaded update and consumes the native updater resource on success.
   * Do not call close() afterwards; close() is only for an unfinished session.
   */
  install: () => Promise<void>;

  /** Releases an unfinished updater resource. Safe to call repeatedly. */
  close: () => Promise<void>;
};

export type AppUpdaterGateway = {
  getCurrentVersion: () => Promise<string>;

  checkForUpdate: () => Promise<AppUpdateHandle | null>;

  relaunch: () => Promise<void>;
};
