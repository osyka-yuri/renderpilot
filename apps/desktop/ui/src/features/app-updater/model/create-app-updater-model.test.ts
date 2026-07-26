import { describe, expect, it, vi } from 'vitest';

import type { AppUpdateDownloadEvent, AppUpdateHandle } from '../api/app-updater-gateway';
import { createGateway, createHandle, createModel, deferred } from './app-updater-test-fixtures';
import type { AppUpdaterModel } from './types';

describe('createAppUpdaterModel', () => {
  describe('startup advisory check', () => {
    it('loads the app version and silently ignores an up-to-date result', async () => {
      const gateway = createGateway();
      const { model, notifyError, notifySuccess } = createModel(gateway);

      await model.start();

      expect(model.appVersion).toBe('1.0.0');
      expect(model.notice).toBeNull();
      expect(model.dialog).toBeNull();
      expect(model.settingsAction).toBe('check');
      expect(notifySuccess).not.toHaveBeenCalled();
      expect(notifyError).not.toHaveBeenCalled();
    });

    it('turns a background result into a passive notice and closes its handle', async () => {
      const handle = createHandle();
      const gateway = createGateway({
        checkForUpdate: vi.fn(() => Promise.resolve(handle)),
      });
      const { model } = createModel(gateway);

      await model.start();

      expect(model.notice?.offer.version).toBe('1.1.0');
      expect(model.dialog).toBeNull();
      expect(model.settingsAction).toBe('open-update');
      expect(handle.close).toHaveBeenCalledTimes(1);
    });

    it('runs its startup probes only once, including concurrent start calls', async () => {
      const version = deferred<string>();
      const update = deferred<AppUpdateHandle | null>();
      const getCurrentVersion = vi.fn(() => version.promise);
      const checkForUpdate = vi.fn(() => update.promise);
      const gateway = createGateway({
        getCurrentVersion,
        checkForUpdate,
      });
      const { model } = createModel(gateway);

      const firstStart = model.start();
      const secondStart = model.start();

      expect(getCurrentVersion).toHaveBeenCalledTimes(1);
      expect(checkForUpdate).toHaveBeenCalledTimes(1);

      version.resolve('1.0.0');
      update.resolve(null);
      await Promise.all([firstStart, secondStart]);
    });

    it('keeps background check errors silent', async () => {
      const gateway = createGateway({
        checkForUpdate: vi.fn(() => Promise.reject(new Error('network'))),
      });
      const { model, notifyError, notifySuccess } = createModel(gateway);
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);

      await model.start();

      expect(model.notice).toBeNull();
      expect(notifySuccess).not.toHaveBeenCalled();
      expect(notifyError).not.toHaveBeenCalled();
      warn.mockRestore();
    });

    it('does not lose the app version when a manual check supersedes startup', async () => {
      const version = deferred<string>();
      const backgroundCheck = deferred<AppUpdateHandle | null>();
      const gateway = createGateway({
        getCurrentVersion: vi.fn(() => version.promise),
        checkForUpdate: vi
          .fn()
          .mockImplementationOnce(() => backgroundCheck.promise)
          .mockResolvedValueOnce(null),
      });
      const { model, notifySuccess } = createModel(gateway);

      const start = model.start();
      const manualCheck = model.checkForUpdates();
      version.resolve('1.0.0');
      backgroundCheck.resolve(null);
      await Promise.all([start, manualCheck]);

      expect(model.appVersion).toBe('1.0.0');
      expect(notifySuccess).toHaveBeenCalledTimes(1);
    });

    it('closes a stale background handle when manual checking begins', async () => {
      const backgroundCheck = deferred<AppUpdateHandle | null>();
      const manualCheck = deferred<AppUpdateHandle | null>();
      const staleHandle = createHandle({ version: '1.1.0' });
      const liveHandle = createHandle({ version: '1.2.0' });
      const gateway = createGateway({
        checkForUpdate: vi
          .fn()
          .mockImplementationOnce(() => backgroundCheck.promise)
          .mockImplementationOnce(() => manualCheck.promise),
      });
      const { model } = createModel(gateway);

      const start = model.start();
      const interactive = model.checkForUpdates();
      backgroundCheck.resolve(staleHandle);
      manualCheck.resolve(liveHandle);
      await Promise.all([start, interactive]);

      expect(staleHandle.close).toHaveBeenCalledTimes(1);
      expect(model.dialog?.phase).toBe('available');
      expect(model.dialog?.offer.version).toBe('1.2.0');
    });
  });

  describe('interactive checks and notices', () => {
    it('opens a live dialog for a manual result and keeps its handle', async () => {
      const handle = createHandle();
      const gateway = createGateway({
        checkForUpdate: vi.fn(() => Promise.resolve(handle)),
      });
      const { model } = createModel(gateway);

      await model.checkForUpdates();

      expect(model.notice?.offer.version).toBe('1.1.0');
      expect(model.dialog?.phase).toBe('available');
      expect(model.dialog?.offer.releaseNotes.blocks.length).toBeGreaterThan(0);
      expect(handle.close).not.toHaveBeenCalled();
    });

    it('uses a fresh interactive check when opening a passive notice', async () => {
      const advisoryHandle = createHandle({ version: '1.1.0' });
      const sessionHandle = createHandle({ version: '1.2.0' });
      const gateway = createGateway({
        checkForUpdate: vi
          .fn()
          .mockResolvedValueOnce(advisoryHandle)
          .mockResolvedValueOnce(sessionHandle),
      });
      const { model } = createModel(gateway);
      await model.start();

      await model.openAvailableUpdate();

      expect(advisoryHandle.close).toHaveBeenCalledTimes(1);
      expect(model.notice?.offer.version).toBe('1.2.0');
      expect(model.dialog?.offer.version).toBe('1.2.0');
      expect(sessionHandle.close).not.toHaveBeenCalled();
    });

    it('keeps a notice when its fresh interactive check fails', async () => {
      const advisoryHandle = createHandle();
      const gateway = createGateway({
        checkForUpdate: vi
          .fn()
          .mockResolvedValueOnce(advisoryHandle)
          .mockRejectedValueOnce(new Error('network')),
      });
      const { model, notifyError } = createModel(gateway);
      await model.start();

      await model.openAvailableUpdate();

      expect(model.notice?.offer.version).toBe('1.1.0');
      expect(model.dialog).toBeNull();
      expect(notifyError).toHaveBeenCalledTimes(1);
    });

    it('returns a dismissible dialog to a notice and safely closes its handle', async () => {
      const handle = createHandle();
      const gateway = createGateway({
        checkForUpdate: vi.fn(() => Promise.resolve(handle)),
      });
      const { model } = createModel(gateway);
      await model.checkForUpdates();

      await model.dismissDialog();

      expect(handle.close).toHaveBeenCalledTimes(1);
      expect(model.dialog).toBeNull();
      expect(model.notice?.offer.version).toBe('1.1.0');
    });

    it('continues after a cleanup error while dismissing a dialog', async () => {
      const handle = createHandle();
      handle.close.mockRejectedValueOnce(new Error('resource already closed'));
      const gateway = createGateway({
        checkForUpdate: vi.fn(() => Promise.resolve(handle)),
      });
      const { model } = createModel(gateway);
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      await model.checkForUpdates();

      await expect(model.dismissDialog()).resolves.toBeUndefined();

      expect(model.dialog).toBeNull();
      expect(model.notice).not.toBeNull();
      warn.mockRestore();
    });

    it('ignores duplicate interactive checks', async () => {
      const check = deferred<AppUpdateHandle | null>();
      const checkForUpdate = vi.fn(() => check.promise);
      const { model } = createModel(createGateway({ checkForUpdate }));

      const first = model.checkForUpdates();
      const second = model.checkForUpdates();
      check.resolve(null);
      await Promise.all([first, second]);

      expect(checkForUpdate).toHaveBeenCalledTimes(1);
    });
  });

  describe('installation and recovery', () => {
    it('downloads, installs without re-closing the consumed handle and relaunches', async () => {
      const callOrder: string[] = [];
      const handle = createHandle({
        installImpl: () => {
          callOrder.push('install');
          return Promise.resolve();
        },
      });
      const relaunch = vi.fn(() => {
        callOrder.push('relaunch');
        return Promise.resolve();
      });
      const gateway = createGateway({
        checkForUpdate: vi.fn(() => Promise.resolve(handle)),
        relaunch,
      });
      const { model } = createModel(gateway);
      await model.checkForUpdates();

      await model.installAvailableUpdate();

      expect(handle.download).toHaveBeenCalledTimes(1);
      expect(callOrder).toEqual(['install', 'relaunch']);
      expect(handle.close).not.toHaveBeenCalled();
      expect(model.dialog?.phase).toBe('restarting');
      expect(model.notice).toBeNull();
    });

    it('shows completed verifying progress and installing before calling install', async () => {
      const phasesAtInstall: (string | undefined)[] = [];
      const settleFrames: { phase: string | undefined; percent: number | null }[] = [];
      const modelRef: { current: AppUpdaterModel | null } = { current: null };
      const handle = createHandle({
        downloadImpl: (onEvent) => {
          onEvent({ type: 'started', contentLength: 100 });
          onEvent({ type: 'progress', chunkLength: 40 });
          // download() may resolve without Finished; model still paints 100% before install.
          return Promise.resolve();
        },
        installImpl: () => {
          phasesAtInstall.push(modelRef.current?.dialog?.phase);
          return Promise.resolve();
        },
      });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
        {
          settleUiBeforeInstallExit: () => {
            const d = modelRef.current?.dialog;
            settleFrames.push({
              phase: d?.phase,
              percent:
                d?.phase === 'downloading' || d?.phase === 'verifying' ? d.progress.percent : null,
            });
            return Promise.resolve();
          },
        },
      );
      modelRef.current = model;
      await model.checkForUpdates();

      await model.installAvailableUpdate();

      expect(settleFrames).toContainEqual({ phase: 'verifying', percent: 100 });
      expect(settleFrames.some((frame) => frame.phase === 'downloading')).toBe(false);
      expect(settleFrames.some((frame) => frame.phase === 'installing')).toBe(true);
      expect(phasesAtInstall).toEqual(['installing']);
      expect(handle.install).toHaveBeenCalledTimes(1);
    });

    it('skips intermediate settle after Finished and only settles into installing', async () => {
      const settleFrames: (string | undefined)[] = [];
      const modelRef: { current: AppUpdaterModel | null } = { current: null };
      const handle = createHandle({
        downloadImpl: (onEvent) => {
          onEvent({ type: 'started', contentLength: 100 });
          onEvent({ type: 'progress', chunkLength: 100 });
          onEvent({ type: 'finished' });
          return Promise.resolve();
        },
      });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
        {
          settleUiBeforeInstallExit: () => {
            settleFrames.push(modelRef.current?.dialog?.phase);
            return Promise.resolve();
          },
        },
      );
      modelRef.current = model;
      await model.checkForUpdates();

      await model.installAvailableUpdate();

      expect(settleFrames).toEqual(['installing']);
      expect(handle.install).toHaveBeenCalledTimes(1);
    });

    it('automatically retries a failed download and installs after recovery', async () => {
      let attempts = 0;
      const handle = createHandle({
        downloadImpl: (onEvent) => {
          attempts += 1;
          if (attempts === 1) {
            return Promise.reject(new Error('network'));
          }
          onEvent({ type: 'started', contentLength: 10 });
          onEvent({ type: 'progress', chunkLength: 10 });
          onEvent({ type: 'finished' });
          return Promise.resolve();
        },
      });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
      );
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      await model.checkForUpdates();

      await model.installAvailableUpdate();

      expect(attempts).toBe(2);
      expect(handle.install).toHaveBeenCalledTimes(1);
      expect(warn).toHaveBeenCalledTimes(1);
      warn.mockRestore();
    });

    it('shows a manual retry only after every automatic download attempt fails', async () => {
      const handle = createHandle({
        downloadImpl: () => Promise.reject(new Error('network')),
      });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
      );
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
      await model.checkForUpdates();

      await model.installAvailableUpdate();

      expect(handle.download).toHaveBeenCalledTimes(3);
      expect(model.dialog?.phase).toBe('prepare-failed');
      expect(handle.install).not.toHaveBeenCalled();

      await model.retry();

      expect(handle.download).toHaveBeenCalledTimes(6);
      expect(warn).toHaveBeenCalledTimes(4);
      expect(error).toHaveBeenCalledTimes(2);
      warn.mockRestore();
      error.mockRestore();
    });

    it('shows a preparation failure when scheduling an automatic retry fails', async () => {
      const waitError = new Error('timer unavailable');
      const handle = createHandle({
        downloadImpl: () => Promise.reject(new Error('network')),
      });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
        { waitBeforeDownloadRetry: () => Promise.reject(waitError) },
      );
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
      await model.checkForUpdates();

      await model.installAvailableUpdate();

      expect(handle.download).toHaveBeenCalledTimes(1);
      expect(handle.install).not.toHaveBeenCalled();
      expect(model.dialog?.phase).toBe('prepare-failed');
      expect(error).toHaveBeenCalledWith(
        'Failed to wait before retrying update download after attempt 1:',
        waitError,
      );
      warn.mockRestore();
      error.mockRestore();
    });

    it('does not start another download after disposal during a retry delay', async () => {
      const retryWait = deferred<undefined>();
      const handle = createHandle({
        downloadImpl: () => Promise.reject(new Error('network')),
      });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
        { waitBeforeDownloadRetry: () => retryWait.promise },
      );
      await model.checkForUpdates();

      const installation = model.installAvailableUpdate();
      await vi.waitFor(() => {
        expect(model.dialog?.phase).toBe('retrying-download');
      });
      await model.dispose();
      retryWait.resolve(undefined);
      await installation;

      expect(handle.download).toHaveBeenCalledTimes(1);
      expect(handle.install).not.toHaveBeenCalled();
      expect(model.dialog).toBeNull();
    });

    it('retries installation without downloading again', async () => {
      let installs = 0;
      const handle = createHandle({
        installImpl: () => {
          installs += 1;
          return installs === 1 ? Promise.reject(new Error('install failed')) : Promise.resolve();
        },
      });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
      );
      await model.checkForUpdates();

      await model.installAvailableUpdate();
      expect(model.dialog?.phase).toBe('install-failed');
      await model.retry();

      expect(handle.download).toHaveBeenCalledTimes(1);
      expect(installs).toBe(2);
    });

    it('returns a failed install to a fresh-notice path when dismissed', async () => {
      const handle = createHandle({
        installImpl: () => Promise.reject(new Error('install failed')),
      });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
      );
      await model.checkForUpdates();
      await model.installAvailableUpdate();

      await model.dismissDialog();

      expect(handle.close).toHaveBeenCalledTimes(1);
      expect(model.dialog).toBeNull();
      expect(model.notice).not.toBeNull();
    });

    it('does not recreate a notice after installation needs a manual restart', async () => {
      const handle = createHandle();
      const { model } = createModel(
        createGateway({
          checkForUpdate: vi.fn(() => Promise.resolve(handle)),
          relaunch: vi.fn(() => Promise.reject(new Error('relaunch failed'))),
        }),
      );
      await model.checkForUpdates();
      await model.installAvailableUpdate();

      expect(model.dialog?.phase).toBe('restart-required');
      await model.dismissDialog();

      expect(model.dialog).toBeNull();
      expect(model.notice).toBeNull();
    });

    it('retries relaunch from restart-required without reopening a download session', async () => {
      let relaunches = 0;
      const handle = createHandle();
      const { model } = createModel(
        createGateway({
          checkForUpdate: vi.fn(() => Promise.resolve(handle)),
          relaunch: vi.fn(() => {
            relaunches += 1;
            return relaunches === 1
              ? Promise.reject(new Error('relaunch failed'))
              : Promise.resolve();
          }),
        }),
      );
      await model.checkForUpdates();
      await model.installAvailableUpdate();
      expect(model.dialog?.phase).toBe('restart-required');

      await model.restartApplication();

      expect(relaunches).toBe(2);
      expect(model.dialog?.phase).toBe('restarting');
      expect(handle.download).toHaveBeenCalledTimes(1);
    });

    it('ignores dialog dismissal during an active download', async () => {
      const download = deferred<undefined>();
      const handle = createHandle({ downloadImpl: () => download.promise });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
      );
      await model.checkForUpdates();

      const install = model.installAvailableUpdate();
      expect(model.dialog?.phase).toBe('downloading');
      await model.dismissDialog();

      expect(model.dialog?.phase).toBe('downloading');
      expect(handle.close).not.toHaveBeenCalled();
      download.resolve(undefined);
      await install;
    });

    it('ignores stale progress and installation completion after disposal', async () => {
      let emit!: (event: AppUpdateDownloadEvent) => void;
      const download = deferred<undefined>();
      const handle = createHandle({
        downloadImpl: (onEvent) => {
          emit = onEvent;
          return download.promise;
        },
      });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
      );
      await model.checkForUpdates();

      const install = model.installAvailableUpdate();
      await model.dispose();
      emit({ type: 'started', contentLength: 50 });
      download.resolve(undefined);
      await install;

      expect(model.dialog).toBeNull();
      expect(handle.install).not.toHaveBeenCalled();
    });

    it('does not re-set dialog when disposed during pre-install settle', async () => {
      const modelRef: { current: AppUpdaterModel | null } = { current: null };
      const handle = createHandle({
        downloadImpl: (onEvent) => {
          onEvent({ type: 'started', contentLength: 10 });
          onEvent({ type: 'progress', chunkLength: 10 });
          onEvent({ type: 'finished' });
          return Promise.resolve();
        },
      });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
        {
          settleUiBeforeInstallExit: async () => {
            await modelRef.current?.dispose();
          },
        },
      );
      modelRef.current = model;
      await model.checkForUpdates();

      await model.installAvailableUpdate();

      expect(model.dialog).toBeNull();
      expect(handle.install).not.toHaveBeenCalled();
    });

    it('releases an installation handle only when installation fails after disposal', async () => {
      const installation = deferred<undefined>();
      const handle = createHandle({ installImpl: () => installation.promise });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
      );
      const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
      await model.checkForUpdates();

      const install = model.installAvailableUpdate();
      // Download + completed-progress paint + installing paint before install awaits.
      await vi.waitFor(() => {
        expect(model.dialog?.phase).toBe('installing');
      });
      await model.dispose();
      installation.reject(new Error('install failed'));
      await install;

      expect(handle.close).toHaveBeenCalledTimes(1);
      expect(model.dialog).toBeNull();
      error.mockRestore();
    });
  });

  describe('disposal', () => {
    it('closes a stale interactive result after disposal', async () => {
      const check = deferred<AppUpdateHandle | null>();
      const handle = createHandle();
      const { model } = createModel(createGateway({ checkForUpdate: vi.fn(() => check.promise) }));

      const pending = model.checkForUpdates();
      await model.dispose();
      check.resolve(handle);
      await pending;

      expect(handle.close).toHaveBeenCalledTimes(1);
      expect(model.dialog).toBeNull();
    });

    it('is idempotent and closes a live session once', async () => {
      const handle = createHandle();
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
      );
      await model.checkForUpdates();

      await model.dispose();
      await model.dispose();

      expect(handle.close).toHaveBeenCalledTimes(1);
    });
  });
});
