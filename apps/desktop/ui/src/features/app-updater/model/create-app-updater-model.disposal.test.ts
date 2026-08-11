import { describe, expect, it, vi } from 'vitest';

import type {
  AppUpdateDownloadEvent,
  AppUpdateHandle,
  AppUpdateInstallOutcome,
} from '../api/app-updater-gateway';
import { createGateway, createHandle, createModel } from './app-updater-test-fixtures';
import type { AppUpdaterModel } from './types';

describe('createAppUpdaterModel', () => {
  describe('disposal', () => {
    it('does not start another download after disposal during a retry delay', async () => {
      const retryWait = Promise.withResolvers<undefined>();
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

    it('ignores stale progress and installation completion after disposal', async () => {
      let emit!: (event: AppUpdateDownloadEvent) => void;
      const download = Promise.withResolvers<undefined>();
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
      const installation = Promise.withResolvers<AppUpdateInstallOutcome>();
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

    it('closes a stale interactive result after disposal', async () => {
      const check = Promise.withResolvers<AppUpdateHandle | null>();
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
