import { t } from '@shared/i18n';
import { ClientError, reportClientError } from '@shared/errors';
import { publishErrorNotification, publishSuccessNotification } from '@shared/notifications';
import { createDisposableRequestChannel } from '@shared/requests';

import type {
  AppUpdateHandle,
  AppUpdateInstallOutcome,
  AppUpdaterGateway,
} from '../api/app-updater-gateway';
import { canDismissDialog } from './dialog-view';
import {
  downloadWithRetries,
  waitForDownloadRetry,
  type DownloadRetryFailure,
} from './download-retry';
import { settleUiBeforeInstallExit as defaultSettleUiBeforeInstallExit } from './install-exit-settle';
import { toOffer } from './offer';
import { toCompletedProgressView, toProgressView, type DownloadProgressState } from './progress';
import type {
  AppUpdateDialogState,
  AppUpdateNotice,
  AppUpdateOffer,
  AppUpdaterModel,
  SettingsUpdateAction,
} from './types';

export type CreateAppUpdaterModelOptions = {
  gateway: AppUpdaterGateway;
  /** Override for tests; defaults to the shared notification bus. */
  notifySuccess?: (message: string) => void;
  notifyError?: (message: string) => void;
  /**
   * Yields so the dialog can paint completed download / installing state before
   * the native apply boundary may exit the process. Defaults to production
   * settle; tests inject a no-op.
   */
  settleUiBeforeInstallExit?: () => Promise<void>;
  /** Overrides the automatic download-retry delay in tests. */
  waitBeforeDownloadRetry?: (delayMs: number) => Promise<void>;
};

export function createAppUpdaterModel(options: CreateAppUpdaterModelOptions): AppUpdaterModel {
  const { gateway } = options;
  const notifySuccess = options.notifySuccess ?? publishSuccessNotification;
  const notifyError = options.notifyError ?? publishErrorNotification;
  const settleUiBeforeInstallExit =
    options.settleUiBeforeInstallExit ?? defaultSettleUiBeforeInstallExit;
  const waitBeforeDownloadRetry = options.waitBeforeDownloadRetry ?? waitForDownloadRetry;

  let appVersion = $state<string | null>(null);
  let notice = $state<AppUpdateNotice | null>(null);
  let dialog = $state<AppUpdateDialogState | null>(null);
  let interactiveChecking = $state(false);
  let pendingUpdate: AppUpdateHandle | null = null;
  let disposed = false;
  let startup: Promise<void> | null = null;
  const operations = createDisposableRequestChannel(() => disposed);

  function beginOperation(): number {
    return operations.begin();
  }

  function isCurrentOperation(id: number): boolean {
    return !operations.isDisposed() && operations.isActive(id);
  }

  async function closeHandle(handle: AppUpdateHandle | null): Promise<void> {
    if (!handle) {
      return;
    }

    try {
      await handle.close();
    } catch (error) {
      // Resource cleanup must never strand the UI in an old updater state.
      reportClientError('updater_close_resource', new ClientError('updater_cleanup_failed', error));
    }
  }

  async function releasePendingHandle(): Promise<void> {
    const handle = pendingUpdate;
    pendingUpdate = null;
    await closeHandle(handle);
  }

  function takePendingHandle(expected: AppUpdateHandle): AppUpdateHandle | null {
    if (pendingUpdate !== expected) {
      return null;
    }

    pendingUpdate = null;
    return expected;
  }

  function showNotice(offer: AppUpdateOffer): void {
    notice = { offer };
  }

  function clearDialog(): void {
    dialog = null;
  }

  function canStartInteractiveCheck(): boolean {
    return !disposed && !interactiveChecking && dialog === null;
  }

  async function loadAppVersion(): Promise<void> {
    try {
      const version = await gateway.getCurrentVersion();
      if (!disposed) {
        appVersion = version;
      }
    } catch (error) {
      reportClientError(
        'updater_get_app_version',
        new ClientError('updater_version_read_failed', error),
      );
    }
  }

  async function checkForUpdateInBackground(): Promise<void> {
    const id = beginOperation();

    try {
      const handle = await gateway.checkForUpdate();
      if (!isCurrentOperation(id)) {
        await closeHandle(handle);
        return;
      }

      if (!handle) {
        return;
      }

      const offer = toOffer(handle);
      await closeHandle(handle);
      if (isCurrentOperation(id)) {
        showNotice(offer);
      }
    } catch (error) {
      if (isCurrentOperation(id)) {
        reportClientError(
          'updater_background_check',
          new ClientError('updater_check_failed', error),
          'warning',
        );
      }
    }
  }

  async function startInteractiveCheck(): Promise<void> {
    if (!canStartInteractiveCheck()) {
      return;
    }

    const id = beginOperation();
    const previousNotice = notice;
    interactiveChecking = true;

    try {
      const handle = await gateway.checkForUpdate();
      if (!isCurrentOperation(id)) {
        await closeHandle(handle);
        return;
      }

      if (!handle) {
        notice = null;
        notifySuccess(t('settings.about.upToDate'));
        return;
      }

      const offer = toOffer(handle);
      await releasePendingHandle();
      if (!isCurrentOperation(id)) {
        await closeHandle(handle);
        return;
      }

      pendingUpdate = handle;
      showNotice(offer);
      dialog = { phase: 'available', offer };
    } catch (error) {
      reportClientError(
        'updater_interactive_check',
        new ClientError('updater_check_failed', error),
      );
      if (isCurrentOperation(id)) {
        notice = previousNotice;
        notifyError(t('settings.about.updateCheckError'));
      }
    } finally {
      if (isCurrentOperation(id)) {
        interactiveChecking = false;
      }
    }
  }

  async function relaunchOrRequireRestart(id: number, offer: AppUpdateOffer): Promise<void> {
    dialog = { phase: 'restarting', offer };

    try {
      await gateway.relaunch();
    } catch (error) {
      reportClientError('updater_relaunch', new ClientError('updater_relaunch_failed', error));
      if (isCurrentOperation(id)) {
        dialog = { phase: 'restart-required', offer };
      }
    }
  }

  /** Set dialog, settle for paint, return whether the operation is still current. */
  async function paintBusyAndContinue(id: number, next: AppUpdateDialogState): Promise<boolean> {
    if (!isCurrentOperation(id)) {
      return false;
    }
    dialog = next;
    await settleUiBeforeInstallExit();
    return isCurrentOperation(id);
  }

  /**
   * Paint a completed (100%) busy frame then installing before the native
   * apply boundary may exit. Uses `verifying` (not `downloading`) so status does
   * not regress after network finish. When Finished already left us on
   * verifying, snap progress without an extra settle.
   */
  async function paintCompletedDownloadThenInstalling(
    id: number,
    offer: AppUpdateOffer,
    progressState: DownloadProgressState,
  ): Promise<boolean> {
    const completed = toCompletedProgressView(progressState);

    if (!progressState.networkFinished) {
      if (
        !(await paintBusyAndContinue(id, {
          phase: 'verifying',
          offer,
          progress: completed,
        }))
      ) {
        return false;
      }
    } else if (isCurrentOperation(id)) {
      dialog = { phase: 'verifying', offer, progress: completed };
    }

    return paintBusyAndContinue(id, { phase: 'installing', offer });
  }

  async function performInstall(
    id: number,
    handle: AppUpdateHandle,
    offer: AppUpdateOffer,
  ): Promise<void> {
    // Tauri consumes the native update resource on a successful install. Detach it before
    // awaiting so dispose cannot close a resource that is already being consumed.
    if (!takePendingHandle(handle)) {
      return;
    }

    let outcome: AppUpdateInstallOutcome;
    try {
      outcome = await handle.install();
    } catch (error) {
      reportClientError('updater_install', new ClientError('updater_install_failed', error));
      if (isCurrentOperation(id)) {
        pendingUpdate = handle;
        dialog = { phase: 'install-failed', offer };
      } else {
        await closeHandle(handle);
      }
      return;
    }

    if (!isCurrentOperation(id)) {
      return;
    }

    notice = null;
    if (outcome.type === 'native-exit') {
      // Rust owns process exit after acknowledging the supervisor handoff. Keep
      // installing visible and never race that native lifecycle transition.
      return;
    }

    await relaunchOrRequireRestart(id, offer);
  }

  function showDownloadProgress(offer: AppUpdateOffer, progress: DownloadProgressState): void {
    dialog = {
      phase: progress.networkFinished ? 'verifying' : 'downloading',
      offer,
      progress: toProgressView(progress),
    };
  }

  function showDownloadFailure(offer: AppUpdateOffer, failure: DownloadRetryFailure): void {
    const code =
      failure.reason === 'download' ? 'updater_download_failed' : 'updater_retry_wait_failed';
    reportClientError(code, new ClientError(code, failure.error));
    dialog = { phase: 'prepare-failed', offer };
  }

  function showDownloadRetry(offer: AppUpdateOffer, error: unknown): void {
    reportClientError(
      'updater_download_retry',
      new ClientError('updater_download_failed', error),
      'warning',
    );
    dialog = { phase: 'retrying-download', offer };
  }

  async function runDownloadAndInstall(id: number, offer: AppUpdateOffer): Promise<void> {
    const handle = pendingUpdate;
    if (!handle || !isCurrentOperation(id)) {
      return;
    }

    const result = await downloadWithRetries({
      download: (onEvent) => handle.download(onEvent),
      isActive: () => isCurrentOperation(id),
      waitBeforeRetry: waitBeforeDownloadRetry,
      onProgress: (progress) => {
        showDownloadProgress(offer, progress);
      },
      onRetryScheduled: ({ error }) => {
        showDownloadRetry(offer, error);
      },
    });

    if (result.status === 'cancelled') {
      return;
    }

    if (result.status === 'failed') {
      if (isCurrentOperation(id)) {
        showDownloadFailure(offer, result);
      }
      return;
    }

    if (!(await paintCompletedDownloadThenInstalling(id, offer, result.progress))) {
      return;
    }

    await performInstall(id, handle, offer);
  }

  async function runInstallOnly(id: number, offer: AppUpdateOffer): Promise<void> {
    const handle = pendingUpdate;
    if (!handle || !isCurrentOperation(id)) {
      return;
    }

    if (!(await paintBusyAndContinue(id, { phase: 'installing', offer }))) {
      return;
    }

    await performInstall(id, handle, offer);
  }

  async function start(): Promise<void> {
    if (disposed) {
      return;
    }

    startup ??= (async () => {
      await Promise.all([loadAppVersion(), checkForUpdateInBackground()]);
    })();
    await startup;
  }

  async function checkForUpdates(): Promise<void> {
    // The Rust boundary permits only one native check at a time. If startup is
    // already probing in the background, let it finish before creating the
    // interactive session instead of surfacing a transient busy error.
    if (startup) {
      await startup;
    }
    await startInteractiveCheck();
  }

  async function openAvailableUpdate(): Promise<void> {
    if (!notice) {
      return;
    }

    await startInteractiveCheck();
  }

  async function dismissDialog(): Promise<void> {
    const currentDialog = dialog;
    if (disposed || !currentDialog || !canDismissDialog(currentDialog)) {
      return;
    }

    const id = beginOperation();
    await releasePendingHandle();
    if (!isCurrentOperation(id)) {
      return;
    }

    clearDialog();
    if (currentDialog.phase === 'restart-required') {
      notice = null;
    } else {
      showNotice(currentDialog.offer);
    }
  }

  async function installAvailableUpdate(): Promise<void> {
    if (disposed || dialog?.phase !== 'available' || !pendingUpdate) {
      return;
    }

    await runDownloadAndInstall(beginOperation(), dialog.offer);
  }

  async function retry(): Promise<void> {
    if (disposed || !dialog || !pendingUpdate) {
      return;
    }

    const currentDialog = dialog;
    if (currentDialog.phase !== 'prepare-failed' && currentDialog.phase !== 'install-failed') {
      return;
    }

    const id = beginOperation();
    if (currentDialog.phase === 'prepare-failed') {
      await runDownloadAndInstall(id, currentDialog.offer);
    } else {
      await runInstallOnly(id, currentDialog.offer);
    }
  }

  async function restartApplication(): Promise<void> {
    if (disposed || dialog?.phase !== 'restart-required') {
      return;
    }

    await relaunchOrRequireRestart(beginOperation(), dialog.offer);
  }

  async function dispose(): Promise<void> {
    if (disposed) {
      return;
    }

    disposed = true;
    operations.invalidate();
    notice = null;
    clearDialog();
    await releasePendingHandle();
  }

  return {
    get appVersion() {
      return appVersion;
    },
    get notice() {
      return notice;
    },
    get dialog() {
      return dialog;
    },
    get settingsAction(): SettingsUpdateAction {
      if (interactiveChecking) {
        return 'checking';
      }
      if (dialog) {
        return 'busy';
      }
      return notice ? 'open-update' : 'check';
    },
    start,
    checkForUpdates,
    openAvailableUpdate,
    dismissDialog,
    installAvailableUpdate,
    retry,
    restartApplication,
    dispose,
  };
}
