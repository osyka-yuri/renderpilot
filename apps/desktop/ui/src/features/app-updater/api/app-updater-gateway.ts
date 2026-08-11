import type {
  AppUpdateApplyDto,
  AppUpdateDownloadEventDto,
  AppUpdateMetadataDto,
} from './tauri-app-updater-contract';

/**
 * Feature-facing updater contract.
 *
 * Keeps Tauri Update / DownloadEvent / Resource types behind the gateway so
 * the model and UI only deal with serializable, owned shapes.
 */

export type AppUpdateMetadata = AppUpdateMetadataDto;

export type AppUpdateDownloadEvent = AppUpdateDownloadEventDto;

/**
 * Result of a successful native apply operation.
 *
 * `installed` means the installed-build backend completed its native install
 * step and the UI may request a relaunch. `native-exit` means the portable
 * supervisor accepted the handoff and Rust owns process exit; replacement
 * authority never depends on WebView IPC.
 */
export type AppUpdateInstallOutcome = AppUpdateApplyDto;

export type AppUpdateHandle = {
  readonly metadata: AppUpdateMetadata;

  download: (onEvent: (event: AppUpdateDownloadEvent) => void) => Promise<void>;

  /**
   * Applies the downloaded update and consumes the native updater session on success.
   * Do not call close() afterwards; close() is only for an unfinished session.
   */
  install: () => Promise<AppUpdateInstallOutcome>;

  /** Releases an unfinished updater resource. Safe to call repeatedly. */
  close: () => Promise<void>;
};

export type AppUpdaterGateway = {
  getCurrentVersion: () => Promise<string>;

  checkForUpdate: () => Promise<AppUpdateHandle | null>;

  relaunch: () => Promise<void>;
};
