import { vi } from 'vitest';

import type {
  AppUpdateDownloadEvent,
  AppUpdateHandle,
  AppUpdateMetadata,
  AppUpdaterGateway,
} from '../api/app-updater-gateway';
import { createAppUpdaterModel } from './create-app-updater-model.svelte';

export type Deferred<T> = {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason?: unknown) => void;
};

export function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

export type MockUpdateHandle = AppUpdateHandle & {
  download: ReturnType<typeof vi.fn>;
  install: ReturnType<typeof vi.fn>;
  close: ReturnType<typeof vi.fn>;
};

export function createHandle(
  overrides: Partial<AppUpdateMetadata> & {
    downloadImpl?: (onEvent: (event: AppUpdateDownloadEvent) => void) => Promise<void>;
    installImpl?: () => Promise<void>;
  } = {},
): MockUpdateHandle {
  const {
    downloadImpl = (onEvent) => {
      onEvent({ type: 'started', contentLength: 100 });
      onEvent({ type: 'progress', chunkLength: 100 });
      onEvent({ type: 'finished' });
      return Promise.resolve();
    },
    installImpl = () => Promise.resolve(),
    ...metadataOverrides
  } = overrides;

  const metadata: AppUpdateMetadata = {
    currentVersion: '1.0.0',
    version: '1.1.0',
    date: '2026-07-11T12:00:00Z',
    body: '## Notes\n\n- item',
    ...metadataOverrides,
  };

  return {
    metadata,
    download: vi.fn(downloadImpl),
    install: vi.fn(installImpl),
    close: vi.fn(() => Promise.resolve()),
  };
}

export function createGateway(overrides: Partial<AppUpdaterGateway> = {}): AppUpdaterGateway {
  return {
    getCurrentVersion: vi.fn(() => Promise.resolve('1.0.0')),
    checkForUpdate: vi.fn(() => Promise.resolve(null)),
    relaunch: vi.fn(() => Promise.resolve()),
    ...overrides,
  };
}

export function createModel(gateway: AppUpdaterGateway) {
  const notifySuccess = vi.fn();
  const notifyError = vi.fn();
  const model = createAppUpdaterModel({
    gateway,
    notifySuccess,
    notifyError,
  });
  return { model, notifySuccess, notifyError };
}
