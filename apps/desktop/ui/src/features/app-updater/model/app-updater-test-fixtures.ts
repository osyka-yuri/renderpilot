import { vi } from 'vitest';

import type {
  AppUpdateDownloadEvent,
  AppUpdateHandle,
  AppUpdateMetadata,
  AppUpdaterGateway,
} from '../api/app-updater-gateway';
import {
  createAppUpdaterModel,
  type CreateAppUpdaterModelOptions,
} from './create-app-updater-model.svelte';

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

function resolveImmediately(): Promise<void> {
  return Promise.resolve();
}

export function createModel(
  gateway: AppUpdaterGateway,
  options: Partial<Omit<CreateAppUpdaterModelOptions, 'gateway'>> = {},
) {
  const {
    notifySuccess = vi.fn(),
    notifyError = vi.fn(),
    settleUiBeforeInstallExit = resolveImmediately,
    waitBeforeDownloadRetry = resolveImmediately,
  } = options;

  const model = createAppUpdaterModel({
    gateway,
    notifySuccess,
    notifyError,
    settleUiBeforeInstallExit,
    waitBeforeDownloadRetry,
  });
  return { model, notifySuccess, notifyError };
}
