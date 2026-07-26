import type { ReleaseNotesDocument } from './release-notes';

export type DownloadProgressView = {
  percent: number | null;
  receivedBytes: number;
  totalBytes: number | null;
  networkFinished: boolean;
};

export type AppUpdateOffer = {
  currentVersion: string;
  version: string;

  /** Original RFC 3339 value, null when absent or invalid. */
  date: string | null;

  /** Already parsed, bounded and safe to render as text nodes. */
  releaseNotes: ReleaseNotesDocument;
};

/**
 * A passive update result. It deliberately has no Tauri update resource, so
 * it can safely live for the rest of the app session.
 */
export type AppUpdateNotice = {
  offer: AppUpdateOffer;
};

export type AppUpdateDialogState =
  | {
      phase: 'available';
      offer: AppUpdateOffer;
    }
  | {
      phase: 'downloading';
      offer: AppUpdateOffer;
      progress: DownloadProgressView;
    }
  | {
      /** Backoff between automatic attempts; stale download progress is hidden. */
      phase: 'retrying-download';
      offer: AppUpdateOffer;
    }
  | {
      phase: 'verifying';
      offer: AppUpdateOffer;
      progress: DownloadProgressView;
    }
  | {
      phase: 'installing';
      offer: AppUpdateOffer;
    }
  | {
      phase: 'restarting';
      offer: AppUpdateOffer;
    }
  | {
      phase: 'prepare-failed';
      offer: AppUpdateOffer;
    }
  | {
      phase: 'install-failed';
      offer: AppUpdateOffer;
    }
  | {
      phase: 'restart-required';
      offer: AppUpdateOffer;
    };

/** The one action the Settings About section can expose at a given time. */
export type SettingsUpdateAction = 'check' | 'checking' | 'open-update' | 'busy';

export type AppUpdaterModel = {
  readonly appVersion: string | null;
  readonly notice: AppUpdateNotice | null;
  readonly dialog: AppUpdateDialogState | null;
  readonly settingsAction: SettingsUpdateAction;

  /** Loads the app version and silently checks for an update once per launch. */
  start(): Promise<void>;
  /** Runs a visible, interactive update check from Settings. */
  checkForUpdates(): Promise<void>;
  /** Refreshes a passive notice into a live install session. */
  openAvailableUpdate(): Promise<void>;
  /** Closes an interactive dialog and returns to a passive notice when appropriate. */
  dismissDialog(): Promise<void>;
  installAvailableUpdate(): Promise<void>;
  retry(): Promise<void>;
  restartApplication(): Promise<void>;
  dispose(): Promise<void>;
};
