import { describe, expect, it, vi } from 'vitest';

import type { AppUpdateHandle } from '../api/app-updater-gateway';
import { createGateway, createHandle, createModel } from './app-updater-test-fixtures';

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
      const version = Promise.withResolvers<string>();
      const update = Promise.withResolvers<AppUpdateHandle | null>();
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

    it('does not lose the app version when a manual check waits for startup', async () => {
      const version = Promise.withResolvers<string>();
      const backgroundCheck = Promise.withResolvers<AppUpdateHandle | null>();
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

    it('closes the background handle before starting a manual check', async () => {
      const backgroundCheck = Promise.withResolvers<AppUpdateHandle | null>();
      const manualCheck = Promise.withResolvers<AppUpdateHandle | null>();
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
      expect(gateway.checkForUpdate).toHaveBeenCalledTimes(1);
      backgroundCheck.resolve(staleHandle);
      manualCheck.resolve(liveHandle);
      await Promise.all([start, interactive]);

      expect(gateway.checkForUpdate).toHaveBeenCalledTimes(2);
      expect(staleHandle.close).toHaveBeenCalledTimes(1);
      expect(model.dialog?.phase).toBe('available');
      expect(model.dialog?.offer.version).toBe('1.2.0');
    });
  });
});
