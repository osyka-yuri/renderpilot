import { describe, expect, it, vi } from 'vitest';

import type { CatalogDelta } from '@entities/game';

import {
  createInitialCatalogLifecycle,
  type CatalogEventListener,
  type CatalogEventPayloads,
} from './initial-catalog-lifecycle';

function flushTasks(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function createHarness(
  started = true,
  rejectedEvent: keyof CatalogEventPayloads | null = null,
  partialFailureCount = 0,
) {
  const handlers = new Map<string, (payload: unknown) => void>();
  const stops = new Map<string, ReturnType<typeof vi.fn>>();
  const listenEvent: CatalogEventListener = (event, handler) => {
    if (event === rejectedEvent) {
      return Promise.reject(new Error(`failed to register ${event}`));
    }
    handlers.set(event, handler as (payload: unknown) => void);
    const stop = vi.fn();
    stops.set(event, stop);
    return Promise.resolve(stop);
  };
  const deps = {
    previewMode: false,
    listenEvent,
    startBackgroundRefresh: vi.fn(() => Promise.resolve({ started, partialFailureCount })),
    startUpdater: vi.fn(),
    onCatalogDelta: vi.fn(),
    onPartialScanFailures: vi.fn(),
    completeInitialCatalogSync: vi.fn(() => Promise.resolve()),
    enableCoverHydration: vi.fn(),
    reportError: vi.fn(),
  };

  return { deps, handlers, stops };
}

describe('createInitialCatalogLifecycle', () => {
  it('registers listeners before starting background services', async () => {
    const { deps, handlers } = createHarness();
    const lifecycle = createInitialCatalogLifecycle(deps);

    expect(handlers.has('catalog://delta')).toBe(true);
    expect(handlers.has('catalog://sync-state')).toBe(true);
    expect(deps.startBackgroundRefresh).not.toHaveBeenCalled();

    lifecycle.startServices();
    await flushTasks();

    expect(deps.startUpdater).toHaveBeenCalledOnce();
    expect(deps.startBackgroundRefresh).toHaveBeenCalledOnce();
    expect(deps.enableCoverHydration).toHaveBeenCalledOnce();
  });

  it('routes deltas and deduplicates command and ready completion', async () => {
    const { deps, handlers } = createHarness();
    const lifecycle = createInitialCatalogLifecycle(deps);
    const delta: CatalogDelta = {
      revision: 3,
      reasons: ['scan'],
      changedGameIds: ['game'],
      removedGameIds: [],
    };

    lifecycle.startServices();
    await flushTasks();
    handlers.get('catalog://delta')?.(delta);
    handlers.get('catalog://sync-state')?.('ready');
    await flushTasks();

    expect(deps.onCatalogDelta).toHaveBeenCalledWith(delta);
    expect(deps.completeInitialCatalogSync).toHaveBeenCalledWith({
      forceCatalogRefresh: false,
    });
    expect(deps.completeInitialCatalogSync).toHaveBeenCalledOnce();
  });

  it('uses completion fallback when startup is already claimed', async () => {
    const { deps } = createHarness(false);
    const lifecycle = createInitialCatalogLifecycle(deps);

    lifecycle.startServices();
    await flushTasks();

    expect(deps.completeInitialCatalogSync).toHaveBeenCalledWith({
      forceCatalogRefresh: false,
    });
    expect(deps.enableCoverHydration).toHaveBeenCalledOnce();
  });

  it('reports partial scan failures from the completed background refresh', async () => {
    const { deps } = createHarness(true, null, 2);
    const lifecycle = createInitialCatalogLifecycle(deps);

    lifecycle.startServices();
    await flushTasks();

    expect(deps.onPartialScanFailures).toHaveBeenCalledOnce();
    expect(deps.onPartialScanFailures).toHaveBeenCalledWith(2);
  });

  it('uses completion fallback and enables covers when startup fails', async () => {
    const { deps } = createHarness();
    const error = new Error('startup failed');
    deps.startBackgroundRefresh.mockRejectedValueOnce(error);
    const lifecycle = createInitialCatalogLifecycle(deps);

    lifecycle.startServices();
    await flushTasks();

    expect(deps.reportError).toHaveBeenCalledWith(
      'Failed to start background catalog refresh.',
      error,
    );
    expect(deps.completeInitialCatalogSync).toHaveBeenCalledWith({
      forceCatalogRefresh: false,
    });
    expect(deps.enableCoverHydration).toHaveBeenCalledOnce();
  });

  it('falls back after a sync-state listener registration failure', async () => {
    const { deps } = createHarness(true, 'catalog://sync-state');
    const lifecycle = createInitialCatalogLifecycle(deps);

    lifecycle.startServices();
    await flushTasks();

    expect(deps.startBackgroundRefresh).toHaveBeenCalledOnce();
    expect(deps.completeInitialCatalogSync).toHaveBeenCalledWith({
      forceCatalogRefresh: false,
    });
    expect(deps.enableCoverHydration).toHaveBeenCalledOnce();
    expect(deps.reportError).toHaveBeenCalledWith(
      'Failed to listen for catalog sync state.',
      expect.any(Error),
    );
  });

  it('forces a catalog refresh when deltas cannot be observed', async () => {
    const { deps, handlers } = createHarness(true, 'catalog://delta');
    const lifecycle = createInitialCatalogLifecycle(deps);

    lifecycle.startServices();
    await flushTasks();
    handlers.get('catalog://sync-state')?.('ready');
    await flushTasks();

    expect(deps.completeInitialCatalogSync).toHaveBeenCalledWith({
      forceCatalogRefresh: true,
    });
  });

  it('stops retained listeners on dispose', async () => {
    const { deps, stops } = createHarness();
    const lifecycle = createInitialCatalogLifecycle(deps);
    await flushTasks();

    lifecycle.dispose();

    expect(stops.get('catalog://delta')).toHaveBeenCalledOnce();
    expect(stops.get('catalog://sync-state')).toHaveBeenCalledOnce();
  });
});
