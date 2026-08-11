import { describe, expect, it, vi } from 'vitest';

import { createGateway, createHandle, createModel } from './app-updater-test-fixtures';
import type { AppUpdaterModel } from './types';

describe('createAppUpdaterModel', () => {
  describe('installation and recovery', () => {
    it('downloads, installs without re-closing the consumed handle and relaunches', async () => {
      const callOrder: string[] = [];
      const handle = createHandle({
        installImpl: () => {
          callOrder.push('install');
          return Promise.resolve({ type: 'installed' });
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
      const settleFrames: { phase: string | undefined; ratio: number | null }[] = [];
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
          return Promise.resolve({ type: 'installed' });
        },
      });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
        {
          settleUiBeforeInstallExit: () => {
            const d = modelRef.current?.dialog;
            settleFrames.push({
              phase: d?.phase,
              ratio:
                d?.phase === 'downloading' || d?.phase === 'verifying' ? d.progress.ratio : null,
            });
            return Promise.resolve();
          },
        },
      );
      modelRef.current = model;
      await model.checkForUpdates();

      await model.installAvailableUpdate();

      expect(settleFrames).toContainEqual({ phase: 'verifying', ratio: 1 });
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
        '[RenderPilot diagnostic]',
        {
          source: 'client-boundary',
          operation: 'updater_retry_wait_failed',
          code: 'updater_retry_wait_failed',
          contractStatus: 'known',
          severity: 'error',
        },
        expect.objectContaining({
          code: 'updater_retry_wait_failed',
          cause: waitError,
        }),
      );
      warn.mockRestore();
      error.mockRestore();
    });

    it('retries installation without downloading again', async () => {
      let installs = 0;
      const handle = createHandle({
        installImpl: () => {
          installs += 1;
          return installs === 1
            ? Promise.reject(new Error('install failed'))
            : Promise.resolve({ type: 'installed' });
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

    it('keeps installing when the portable native handoff owns process exit', async () => {
      const handle = createHandle({
        installImpl: () => Promise.resolve({ type: 'native-exit' }),
      });
      const relaunch = vi.fn(() => Promise.resolve());
      const { model } = createModel(
        createGateway({
          checkForUpdate: vi.fn(() => Promise.resolve(handle)),
          relaunch,
        }),
      );
      await model.checkForUpdates();

      await model.installAvailableUpdate();

      expect(relaunch).not.toHaveBeenCalled();
      expect(model.dialog?.phase).toBe('installing');
      expect(model.notice).toBeNull();
    });

    it('ignores a stale retry without invalidating an active download', async () => {
      const download = Promise.withResolvers<undefined>();
      const handle = createHandle({ downloadImpl: () => download.promise });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
      );
      await model.checkForUpdates();

      const installation = model.installAvailableUpdate();
      expect(model.dialog?.phase).toBe('downloading');
      await model.retry();
      download.resolve(undefined);
      await installation;

      expect(handle.install).toHaveBeenCalledTimes(1);
      expect(model.dialog?.phase).toBe('restarting');
    });

    it('ignores a double retry after the first retry starts downloading', async () => {
      const retryDownload = Promise.withResolvers<undefined>();
      let downloads = 0;
      const handle = createHandle({
        downloadImpl: () => {
          downloads += 1;
          return downloads <= 3 ? Promise.reject(new Error('network')) : retryDownload.promise;
        },
      });
      const { model } = createModel(
        createGateway({ checkForUpdate: vi.fn(() => Promise.resolve(handle)) }),
      );
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
      await model.checkForUpdates();
      await model.installAvailableUpdate();
      expect(model.dialog?.phase).toBe('prepare-failed');

      const firstRetry = model.retry();
      const staleRetry = model.retry();
      expect(model.dialog?.phase).toBe('downloading');
      retryDownload.resolve(undefined);
      await Promise.all([firstRetry, staleRetry]);

      expect(handle.download).toHaveBeenCalledTimes(4);
      expect(handle.install).toHaveBeenCalledTimes(1);
      expect(model.dialog?.phase).toBe('restarting');
      warn.mockRestore();
      error.mockRestore();
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
      const download = Promise.withResolvers<undefined>();
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
  });
});
