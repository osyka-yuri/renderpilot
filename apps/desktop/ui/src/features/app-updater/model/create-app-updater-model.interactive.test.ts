import { describe, expect, it, vi } from 'vitest';

import type { AppUpdateHandle } from '../api/app-updater-gateway';
import { createGateway, createHandle, createModel } from './app-updater-test-fixtures';

describe('createAppUpdaterModel', () => {
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
      const check = Promise.withResolvers<AppUpdateHandle | null>();
      const checkForUpdate = vi.fn(() => check.promise);
      const { model } = createModel(createGateway({ checkForUpdate }));

      const first = model.checkForUpdates();
      const second = model.checkForUpdates();
      check.resolve(null);
      await Promise.all([first, second]);

      expect(checkForUpdate).toHaveBeenCalledTimes(1);
    });
  });
});
