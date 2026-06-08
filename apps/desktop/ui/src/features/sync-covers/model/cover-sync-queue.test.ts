import { describe, expect, it, vi } from 'vitest';

import { createCoverSyncQueue } from './cover-sync-queue.svelte';

describe('createCoverSyncQueue', () => {
  it('runs the queued sync and clears isSyncing when done', async () => {
    const queue = createCoverSyncQueue();
    const syncFn = vi.fn(() => Promise.resolve());

    queue.queue(syncFn, vi.fn());
    await vi.waitFor(() => {
      expect(queue.isSyncing).toBe(false);
    });

    expect(syncFn).toHaveBeenCalledTimes(1);
  });

  it('coalesces calls made while a sync is in flight into a single re-run', async () => {
    const queue = createCoverSyncQueue();
    const onError = vi.fn();
    let calls = 0;

    const syncFn = vi.fn(() => {
      calls += 1;
      if (calls === 1) {
        // Re-queue twice while the first sync is still running; both must
        // collapse into exactly one additional drain pass.
        queue.queue(syncFn, onError);
        queue.queue(syncFn, onError);
      }
      return Promise.resolve();
    });

    queue.queue(syncFn, onError);
    await vi.waitFor(() => {
      expect(queue.isSyncing).toBe(false);
    });

    expect(calls).toBe(2);
    expect(onError).not.toHaveBeenCalled();
  });

  it('forwards a sync failure to the onError callback', async () => {
    const queue = createCoverSyncQueue();
    const boom = new Error('sync boom');
    const onError = vi.fn();

    queue.queue(() => Promise.reject(boom), onError);
    await vi.waitFor(() => {
      expect(onError).toHaveBeenCalledWith(boom);
    });
  });

  it('tracks auto-fetching ids and clears them after a drain', async () => {
    const queue = createCoverSyncQueue();

    queue.setAutoFetching('game-1', true);
    expect(queue.autoFetchingIds.has('game-1')).toBe(true);

    queue.setAutoFetching('game-1', false);
    expect(queue.autoFetchingIds.has('game-1')).toBe(false);

    queue.queue(() => {
      queue.setAutoFetching('game-2', true);
      return Promise.resolve();
    }, vi.fn());
    await vi.waitFor(() => {
      expect(queue.isSyncing).toBe(false);
    });

    expect(queue.autoFetchingIds.size).toBe(0);
  });

  it('clears all auto-fetching ids on demand', () => {
    const queue = createCoverSyncQueue();

    queue.setAutoFetching('a', true);
    queue.setAutoFetching('b', true);
    queue.clearAutoFetching();

    expect(queue.autoFetchingIds.size).toBe(0);
  });
});
