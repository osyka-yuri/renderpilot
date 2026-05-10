import { setCatalogSetting } from '@shared/api/desktop';
import {
  commitPersistedSnapshot,
  createPersistedSnapshot,
  isPersistedSnapshotStillCurrent,
  type GamesFilterState,
} from '@features/games/games-filter-state';
import { GAMES_FILTERS_CATALOG_SETTING_KEY } from '@features/games/games-screen-filters';

const DEFAULT_SEARCH_PERSIST_DEBOUNCE_MS = 250;

export type GamesFilterPersistenceContext = {
  getState: () => GamesFilterState;
  setState: (next: GamesFilterState) => void;
};

export type GamesFilterPersistenceOptions = {
  debounceMs?: number;
  onPersistError?: (error: unknown) => void;
};

function defaultPersistErrorHandler(error: unknown): void {
  console.error('Failed to persist game filters.', error);
}

export function createGamesFilterPersistence(options?: GamesFilterPersistenceOptions) {
  const debounceMs = options?.debounceMs ?? DEFAULT_SEARCH_PERSIST_DEBOUNCE_MS;
  const onPersistError = options?.onPersistError ?? defaultPersistErrorHandler;

  let persistRunner: Promise<void> | null = null;
  let hasPendingPersist = false;
  let latestContext: GamesFilterPersistenceContext | null = null;
  let searchPersistTimer: ReturnType<typeof setTimeout> | null = null;
  let disposed = false;

  function reportPersistError(error: unknown): void {
    try {
      onPersistError(error);
    } catch (handlerError) {
      console.error('Failed to handle game filters persist error.', handlerError);
    }
  }

  function clearSearchPersistTimer(): void {
    if (searchPersistTimer === null) {
      return;
    }

    clearTimeout(searchPersistTimer);
    searchPersistTimer = null;
  }

  async function persistCurrentState(ctx: GamesFilterPersistenceContext): Promise<void> {
    const state = ctx.getState();

    if (!state.ready) {
      return;
    }

    const snapshot = createPersistedSnapshot(state);

    if (snapshot === state.lastPersistedSnapshot) {
      return;
    }

    try {
      await setCatalogSetting(GAMES_FILTERS_CATALOG_SETTING_KEY, snapshot);

      if (disposed) {
        return;
      }

      const current = ctx.getState();

      if (isPersistedSnapshotStillCurrent(current, snapshot)) {
        ctx.setState(commitPersistedSnapshot(current, snapshot));
      }
    } catch (error) {
      reportPersistError(error);
    }
  }

  async function drainPersistQueue(): Promise<void> {
    while (!disposed && hasPendingPersist) {
      hasPendingPersist = false;

      const ctx = latestContext;

      if (ctx === null) {
        return;
      }

      await persistCurrentState(ctx);
    }
  }

  function requestPersist(ctx: GamesFilterPersistenceContext): Promise<void> {
    if (disposed) {
      return Promise.resolve();
    }

    latestContext = ctx;
    hasPendingPersist = true;

    persistRunner ??= drainPersistQueue().finally(() => {
      persistRunner = null;
    });

    return persistRunner;
  }

  return {
    persistFilters(ctx: GamesFilterPersistenceContext): Promise<void> {
      return requestPersist(ctx);
    },

    queueSearchPersist(ctx: GamesFilterPersistenceContext): void {
      if (disposed || !ctx.getState().ready) {
        return;
      }

      clearSearchPersistTimer();

      searchPersistTimer = setTimeout(() => {
        searchPersistTimer = null;
        void requestPersist(ctx);
      }, debounceMs);
    },

    flushQueuedSearchPersist(ctx: GamesFilterPersistenceContext): void {
      if (disposed || searchPersistTimer === null) {
        return;
      }

      clearSearchPersistTimer();
      void requestPersist(ctx);
    },

    dispose(): void {
      disposed = true;
      hasPendingPersist = false;
      latestContext = null;
      clearSearchPersistTimer();
    },
  };
}
