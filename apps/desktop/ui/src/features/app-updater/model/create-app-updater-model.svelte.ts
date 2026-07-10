import { toast } from 'svelte-sonner';

import { t } from '@shared/i18n';
import { createDisposableRequestChannel } from '@shared/requests';

import type { AppUpdateHandle, AppUpdaterGateway } from '../api/app-updater-gateway';
import { canDismissDialog } from './dialog-view';
import { toOffer } from './offer';
import {
  applyDownloadEvent,
  EMPTY_PROGRESS,
  toProgressView,
  type DownloadProgressState,
} from './progress';
import type {
  AppUpdateDialogState,
  AppUpdateNotice,
  AppUpdateOffer,
  AppUpdaterModel,
  SettingsUpdateAction,
} from './types';

export type CreateAppUpdaterModelOptions = {
  gateway: AppUpdaterGateway;
  /** Override for tests; defaults to svelte-sonner toasts. */
  notifySuccess?: (message: string) => void;
  notifyError?: (message: string) => void;
};

export function createAppUpdaterModel(options: CreateAppUpdaterModelOptions): AppUpdaterModel {
  const { gateway } = options;
  const notifySuccess = options.notifySuccess ?? ((message: string) => toast.success(message));
  const notifyError = options.notifyError ?? ((message: string) => toast.error(message));

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

  async function closeHandle(handle: AppUpdateHandle | null, context: string): Promise<void> {
    if (!handle) {
      return;
    }

    try {
      await handle.close();
    } catch (error) {
      // Resource cleanup must never strand the UI in an old updater state.
      console.warn(`Failed to close updater resource after ${context}`, error);
    }
  }

  async function releasePendingHandle(context: string): Promise<void> {
    const handle = pendingUpdate;
    pendingUpdate = null;
    await closeHandle(handle, context);
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
      console.warn('Failed to get app version', error);
    }
  }

  async function checkForUpdateInBackground(): Promise<void> {
    const id = beginOperation();

    try {
      const handle = await gateway.checkForUpdate();
      if (!isCurrentOperation(id)) {
        await closeHandle(handle, 'a stale background update check');
        return;
      }

      if (!handle) {
        return;
      }

      const offer = toOffer(handle);
      await closeHandle(handle, 'a background update check');
      if (isCurrentOperation(id)) {
        showNotice(offer);
      }
    } catch (error) {
      if (isCurrentOperation(id)) {
        console.warn('Failed to check for updates in the background', error);
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
        await closeHandle(handle, 'a stale interactive update check');
        return;
      }

      if (!handle) {
        notice = null;
        notifySuccess(t('settings.about.upToDate'));
        return;
      }

      const offer = toOffer(handle);
      await releasePendingHandle('replacing an interactive update session');
      if (!isCurrentOperation(id)) {
        await closeHandle(handle, 'a stale interactive update check');
        return;
      }

      pendingUpdate = handle;
      showNotice(offer);
      dialog = { phase: 'available', offer };
    } catch (error) {
      console.error('Failed to check for updates:', error);
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
      console.error('Failed to relaunch application:', error);
      if (isCurrentOperation(id)) {
        dialog = { phase: 'restart-required', offer };
      }
    }
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

    try {
      await handle.install();
    } catch (error) {
      console.error('Failed to install update:', error);
      if (isCurrentOperation(id)) {
        pendingUpdate = handle;
        dialog = { phase: 'install-failed', offer };
      } else {
        await closeHandle(handle, 'a failed installation after disposal');
      }
      return;
    }

    if (!isCurrentOperation(id)) {
      return;
    }

    // An installed update is no longer actionable, even if relaunch fails.
    notice = null;
    await relaunchOrRequireRestart(id, offer);
  }

  async function runDownloadAndInstall(id: number, offer: AppUpdateOffer): Promise<void> {
    const handle = pendingUpdate;
    if (!handle || !isCurrentOperation(id)) {
      return;
    }

    let progressState: DownloadProgressState = { ...EMPTY_PROGRESS };
    dialog = { phase: 'downloading', offer, progress: toProgressView(progressState) };

    try {
      await handle.download((event) => {
        if (!isCurrentOperation(id)) {
          return;
        }

        progressState = applyDownloadEvent(progressState, event);
        dialog = {
          phase: progressState.networkFinished ? 'verifying' : 'downloading',
          offer,
          progress: toProgressView(progressState),
        };
      });

      if (!isCurrentOperation(id)) {
        return;
      }

      dialog = { phase: 'installing', offer };
      await performInstall(id, handle, offer);
    } catch (error) {
      console.error('Failed to download or verify update:', error);
      if (isCurrentOperation(id)) {
        dialog = { phase: 'prepare-failed', offer };
      }
    }
  }

  async function runInstallOnly(id: number, offer: AppUpdateOffer): Promise<void> {
    const handle = pendingUpdate;
    if (!handle || !isCurrentOperation(id)) {
      return;
    }

    dialog = { phase: 'installing', offer };
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
    await releasePendingHandle('dismissing the update dialog');
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

    const id = beginOperation();
    if (dialog.phase === 'prepare-failed') {
      await runDownloadAndInstall(id, dialog.offer);
    } else if (dialog.phase === 'install-failed') {
      await runInstallOnly(id, dialog.offer);
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
    await releasePendingHandle('disposing the app updater');
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
