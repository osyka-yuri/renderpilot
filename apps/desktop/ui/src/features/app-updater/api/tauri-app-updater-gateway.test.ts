import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  check: vi.fn(),
  getVersion: vi.fn(),
  relaunch: vi.fn(),
}));

vi.mock('@tauri-apps/api/app', () => ({ getVersion: mocks.getVersion }));
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: mocks.relaunch }));
vi.mock('@tauri-apps/plugin-updater', () => ({ check: mocks.check }));

import { createTauriAppUpdaterGateway } from './tauri-app-updater-gateway';

function createUpdate() {
  return {
    currentVersion: '1.0.0',
    version: '1.1.0',
    date: undefined,
    body: undefined,
    download: vi.fn((onEvent?: (event: unknown) => void) => {
      onEvent?.({ event: 'Started', data: { contentLength: 10 } });
      onEvent?.({ event: 'Progress', data: { chunkLength: 10 } });
      onEvent?.({ event: 'Finished' });
      return Promise.resolve();
    }),
    install: vi.fn(() => Promise.resolve()),
    close: vi.fn(() => Promise.resolve()),
  };
}

describe('createTauriAppUpdaterGateway', () => {
  beforeEach(() => {
    mocks.check.mockReset();
    mocks.getVersion.mockReset();
    mocks.relaunch.mockReset();
  });

  it('maps the app version and no-update result', async () => {
    mocks.getVersion.mockResolvedValue('1.0.0');
    mocks.check.mockResolvedValue(null);
    const gateway = createTauriAppUpdaterGateway();

    await expect(gateway.getCurrentVersion()).resolves.toBe('1.0.0');
    await expect(gateway.checkForUpdate()).resolves.toBeNull();
  });

  it('maps metadata, lifecycle operations and download events', async () => {
    const update = createUpdate();
    mocks.check.mockResolvedValue(update);
    const gateway = createTauriAppUpdaterGateway();

    const handle = await gateway.checkForUpdate();
    expect(handle?.metadata).toEqual({
      currentVersion: '1.0.0',
      version: '1.1.0',
      date: null,
      body: '',
    });

    const events: unknown[] = [];
    await handle?.download((event) => events.push(event));
    await handle?.install();
    await handle?.close();

    expect(events).toEqual([
      { type: 'started', contentLength: 10 },
      { type: 'progress', chunkLength: 10 },
      { type: 'finished' },
    ]);
    expect(update.install).toHaveBeenCalledTimes(1);
    expect(update.close).not.toHaveBeenCalled();
  });

  it('closes an unfinished update exactly once', async () => {
    const update = createUpdate();
    mocks.check.mockResolvedValue(update);
    const gateway = createTauriAppUpdaterGateway();

    const handle = await gateway.checkForUpdate();
    await handle?.close();
    await handle?.close();

    expect(update.close).toHaveBeenCalledTimes(1);
  });

  it('keeps a failed installation releasable', async () => {
    const update = createUpdate();
    update.install.mockRejectedValueOnce(new Error('install failed'));
    mocks.check.mockResolvedValue(update);
    const gateway = createTauriAppUpdaterGateway();

    const handle = await gateway.checkForUpdate();
    await expect(handle?.install()).rejects.toThrow('install failed');
    await handle?.close();

    expect(update.close).toHaveBeenCalledTimes(1);
  });

  it('forwards relaunch failures to the model', async () => {
    mocks.relaunch.mockRejectedValue(new Error('relaunch failed'));
    const gateway = createTauriAppUpdaterGateway();

    await expect(gateway.relaunch()).rejects.toThrow('relaunch failed');
  });
});
