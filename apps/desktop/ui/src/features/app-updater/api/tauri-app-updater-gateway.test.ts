import { readFile } from 'node:fs/promises';

import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getVersion: vi.fn(),
  invokeDesktop: vi.fn(),
  relaunch: vi.fn(),
  channels: [] as { onmessage: (event: unknown) => void }[],
}));

vi.mock('@tauri-apps/api/app', () => ({ getVersion: mocks.getVersion }));
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {
    onmessage: (event: unknown) => void;

    constructor(onmessage: (event: unknown) => void) {
      this.onmessage = onmessage;
      mocks.channels.push(this);
    }
  },
}));
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: mocks.relaunch }));
vi.mock('@shared/api', () => ({ invokeDesktop: mocks.invokeDesktop }));

import { createTauriAppUpdaterGateway } from './tauri-app-updater-gateway';

function createCheckResult() {
  return {
    sessionId: 'session-1',
    metadata: {
      currentVersion: '1.0.0',
      version: '1.1.0',
      date: null,
      body: '',
    },
  };
}

describe('createTauriAppUpdaterGateway', () => {
  beforeEach(() => {
    mocks.getVersion.mockReset();
    mocks.invokeDesktop.mockReset();
    mocks.relaunch.mockReset();
    mocks.channels.length = 0;
  });

  it('maps the app version and no-update result', async () => {
    mocks.getVersion.mockResolvedValue('1.0.0');
    mocks.invokeDesktop.mockResolvedValue(null);
    const gateway = createTauriAppUpdaterGateway();

    await expect(gateway.getCurrentVersion()).resolves.toBe('1.0.0');
    await expect(gateway.checkForUpdate()).resolves.toBeNull();
    expect(mocks.invokeDesktop).toHaveBeenCalledWith('app_update_check');
  });

  it('maps metadata, lifecycle operations and download events', async () => {
    const checkResult = createCheckResult();
    mocks.invokeDesktop.mockImplementation((command: string, payload?: unknown) => {
      if (command === 'app_update_check') {
        return Promise.resolve(checkResult);
      }

      if (command === 'app_update_download') {
        const channel = (payload as { onEvent: { onmessage: (event: unknown) => void } }).onEvent;
        channel.onmessage({ type: 'started', contentLength: 10 });
        channel.onmessage({ type: 'progress', chunkLength: 10 });
        channel.onmessage({ type: 'finished' });
      }

      if (command === 'app_update_apply') {
        return Promise.resolve({ type: 'installed' });
      }

      return Promise.resolve();
    });
    const gateway = createTauriAppUpdaterGateway();

    const handle = await gateway.checkForUpdate();
    expect(handle?.metadata).toBe(checkResult.metadata);
    expect(handle?.metadata).toEqual({
      currentVersion: '1.0.0',
      version: '1.1.0',
      date: null,
      body: '',
    });

    const events: unknown[] = [];
    await handle?.download((event) => events.push(event));
    await expect(handle?.install()).resolves.toEqual({ type: 'installed' });
    await handle?.close();

    expect(events).toEqual([
      { type: 'started', contentLength: 10 },
      { type: 'progress', chunkLength: 10 },
      { type: 'finished' },
    ]);
    expect(mocks.invokeDesktop).toHaveBeenNthCalledWith(2, 'app_update_download', {
      sessionId: 'session-1',
      onEvent: mocks.channels[0],
    });
    expect(mocks.invokeDesktop).toHaveBeenNthCalledWith(3, 'app_update_apply', {
      sessionId: 'session-1',
    });
    expect(mocks.invokeDesktop).not.toHaveBeenCalledWith('app_update_close', {
      sessionId: 'session-1',
    });
  });

  it('closes an unfinished update exactly once', async () => {
    mocks.invokeDesktop.mockResolvedValueOnce(createCheckResult()).mockResolvedValue(undefined);
    const gateway = createTauriAppUpdaterGateway();

    const handle = await gateway.checkForUpdate();
    await handle?.close();
    await handle?.close();

    expect(mocks.invokeDesktop).toHaveBeenCalledTimes(2);
    expect(mocks.invokeDesktop).toHaveBeenLastCalledWith('app_update_close', {
      sessionId: 'session-1',
    });
  });

  it('waits for an active download before closing its final Rust session state', async () => {
    const download = Promise.withResolvers<undefined>();
    mocks.invokeDesktop.mockImplementation((command: string) => {
      if (command === 'app_update_check') {
        return Promise.resolve(createCheckResult());
      }
      if (command === 'app_update_download') {
        return download.promise;
      }
      return Promise.resolve();
    });
    const gateway = createTauriAppUpdaterGateway();
    const handle = await gateway.checkForUpdate();

    const downloading = handle?.download(() => undefined);
    const closing = handle?.close();
    await Promise.resolve();
    expect(mocks.invokeDesktop).not.toHaveBeenCalledWith('app_update_close', {
      sessionId: 'session-1',
    });

    download.resolve(undefined);
    await downloading;
    await closing;
    expect(mocks.invokeDesktop).toHaveBeenLastCalledWith('app_update_close', {
      sessionId: 'session-1',
    });
  });

  it('keeps a failed installation releasable', async () => {
    mocks.invokeDesktop
      .mockResolvedValueOnce(createCheckResult())
      .mockRejectedValueOnce(new Error('install failed'))
      .mockResolvedValueOnce(undefined);
    const gateway = createTauriAppUpdaterGateway();

    const handle = await gateway.checkForUpdate();
    await expect(handle?.install()).rejects.toThrow('install failed');
    await handle?.close();

    expect(mocks.invokeDesktop).toHaveBeenLastCalledWith('app_update_close', {
      sessionId: 'session-1',
    });
  });

  it('leaves portable process exit entirely inside the Rust handoff', async () => {
    mocks.invokeDesktop.mockImplementation((command: string) => {
      if (command === 'app_update_check') {
        return Promise.resolve(createCheckResult());
      }
      if (command === 'app_update_apply') {
        return Promise.resolve({ type: 'native-exit' });
      }
      return Promise.resolve();
    });
    const gateway = createTauriAppUpdaterGateway();
    const handle = await gateway.checkForUpdate();

    await expect(handle?.install()).resolves.toEqual({ type: 'native-exit' });
    await expect(handle?.close()).resolves.toBeUndefined();
    expect(mocks.relaunch).not.toHaveBeenCalled();
    expect(mocks.invokeDesktop).not.toHaveBeenCalledWith('app_update_close', {
      sessionId: 'session-1',
    });
  });

  it('forwards relaunch failures to the model', async () => {
    mocks.relaunch.mockRejectedValue(new Error('relaunch failed'));
    const gateway = createTauriAppUpdaterGateway();

    await expect(gateway.relaunch()).rejects.toThrow('relaunch failed');
  });

  it('keeps portable trial readiness outside the updater DTO gateway', async () => {
    const [gatewaySource, desktopSource] = await Promise.all([
      readFile(new URL('./tauri-app-updater-gateway.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../app/routes/DesktopApp.svelte', import.meta.url), 'utf8'),
    ]);

    expect(gatewaySource).not.toContain('portable_trial_ready');
    expect(gatewaySource).not.toContain('app_update_confirm_started');
    expect(gatewaySource).not.toContain('journal');
    expect(desktopSource).toContain("invokeDesktop('portable_trial_ready')");
  });

  it('does not import the updater plugin into the UI bundle', async () => {
    const source = await readFile(
      new URL('./tauri-app-updater-gateway.ts', import.meta.url),
      'utf8',
    );

    expect(source).not.toContain('@tauri-apps/plugin-updater');
  });
});
