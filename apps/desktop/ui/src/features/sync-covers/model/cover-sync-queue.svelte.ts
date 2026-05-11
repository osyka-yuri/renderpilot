import { SvelteSet } from 'svelte/reactivity';

export type CoverSyncQueue = ReturnType<typeof createCoverSyncQueue>;

export function createCoverSyncQueue() {
  let syncing = $state(false);
  let queued = $state(false);
  const autoFetchingIds = new SvelteSet<string>();

  function queue(syncFn: () => Promise<void>, onError: (error: unknown) => void): void {
    queued = true;

    if (syncing) {
      return;
    }

    void drain(syncFn).catch(onError);
  }

  async function drain(syncFn: () => Promise<void>): Promise<void> {
    if (syncing) {
      return;
    }

    syncing = true;

    try {
      while (queued) {
        queued = false;
        await syncFn();
      }
    } finally {
      autoFetchingIds.clear();
      syncing = false;
    }
  }

  function setAutoFetching(gameId: string, isFetching: boolean): void {
    if (isFetching) {
      autoFetchingIds.add(gameId);
    } else {
      autoFetchingIds.delete(gameId);
    }
  }

  function clearAutoFetching(): void {
    autoFetchingIds.clear();
  }

  return {
    get isSyncing() {
      return syncing;
    },
    get isQueued() {
      return queued;
    },
    get autoFetchingIds() {
      return autoFetchingIds;
    },
    queue,
    setAutoFetching,
    clearAutoFetching,
  };
}
