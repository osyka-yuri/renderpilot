import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  createInitialGamesFilterState,
  hydrateGamesFilterState,
  withSearchQuery,
  type GamesFilterState,
} from './games-filter-state';
import {
  createGamesFilterPersistence,
  type GamesFilterPersistenceOptions,
} from './games-filter-persistence';

const TEST_SETTING_KEY = 'games_filters_v3';

type SetCatalogSettingMock = ReturnType<
  typeof vi.fn<(key: string, value: string) => Promise<{ saved: boolean }>>
>;

type PersistResult = { saved: boolean };

type Deferred<T> = {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
};

type PersistedFiltersSnapshot = {
  searchQuery?: unknown;
};

function createDeferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;

  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });

  return { promise, resolve, reject };
}

async function flushMicrotasks(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

function createReadyState(searchQuery?: string): GamesFilterState {
  const readyState = hydrateGamesFilterState(
    createInitialGamesFilterState(),
    null,
    ['LibraryAlpha', 'LibraryBeta'],
    [],
  );

  return searchQuery === undefined ? readyState : withSearchQuery(readyState, searchQuery);
}

function createDirtyReadyState(searchQuery = 'dirty'): GamesFilterState {
  return createReadyState(searchQuery);
}

function createPersistenceContext(initialState: GamesFilterState) {
  let state = initialState;

  return {
    getState: () => state,

    setState: vi.fn((next: GamesFilterState) => {
      state = next;
    }),

    getCurrentState: () => state,

    replaceState: (next: GamesFilterState) => {
      state = next;
    },

    updateState: (updater: (current: GamesFilterState) => GamesFilterState) => {
      state = updater(state);
    },
  };
}

function createSuccessfulPersistResult(): PersistResult {
  return { saved: true };
}

describe('games-filter-persistence', () => {
  let setCatalogSettingMock: SetCatalogSettingMock;

  beforeEach(() => {
    vi.useRealTimers();
    setCatalogSettingMock = vi.fn();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  function createPersistence(overrides?: Partial<GamesFilterPersistenceOptions>) {
    return createGamesFilterPersistence({
      setCatalogSetting: setCatalogSettingMock,
      settingKey: TEST_SETTING_KEY,
      ...overrides,
    });
  }

  function mockSuccessfulPersist(): void {
    setCatalogSettingMock.mockResolvedValue(createSuccessfulPersistResult());
  }

  function getPersistedSnapshot(callIndex = 0): string {
    const snapshot = setCatalogSettingMock.mock.calls[callIndex]?.[1];

    expect(typeof snapshot).toBe('string');

    return snapshot;
  }

  function parsePersistedSnapshot(callIndex = 0): PersistedFiltersSnapshot {
    const parsed: unknown = JSON.parse(getPersistedSnapshot(callIndex));

    expect(parsed).toEqual(expect.any(Object));

    return parsed as PersistedFiltersSnapshot;
  }

  function expectPersistedSearchQuery(callIndex: number, searchQuery: string): void {
    expect(parsePersistedSnapshot(callIndex).searchQuery).toBe(searchQuery);
  }

  it('does not persist when state is not ready', async () => {
    const persistence = createPersistence();
    const ctx = createPersistenceContext(createInitialGamesFilterState());

    await persistence.persistFilters(ctx);

    expect(setCatalogSettingMock).not.toHaveBeenCalled();
    expect(ctx.setState).not.toHaveBeenCalled();
  });

  it('persists dirty ready state and commits persisted snapshot', async () => {
    mockSuccessfulPersist();

    const persistence = createPersistence();
    const ctx = createPersistenceContext(createDirtyReadyState('dirty'));

    await persistence.persistFilters(ctx);

    expect(setCatalogSettingMock).toHaveBeenCalledTimes(1);
    expect(ctx.setState).toHaveBeenCalledTimes(1);
  });

  it('does not persist the same committed snapshot twice', async () => {
    mockSuccessfulPersist();

    const persistence = createPersistence();
    const ctx = createPersistenceContext(createDirtyReadyState('dirty'));

    await persistence.persistFilters(ctx);
    await persistence.persistFilters(ctx);

    expect(setCatalogSettingMock).toHaveBeenCalledTimes(1);
    expect(ctx.setState).toHaveBeenCalledTimes(1);
  });

  it('deduplicates concurrent persist requests while write is in flight', async () => {
    const write = createDeferred<PersistResult>();
    setCatalogSettingMock.mockReturnValue(write.promise);

    const persistence = createPersistence();
    const ctx = createPersistenceContext(createDirtyReadyState('dirty'));

    const firstPersist = persistence.persistFilters(ctx);
    const secondPersist = persistence.persistFilters(ctx);

    expect(setCatalogSettingMock).toHaveBeenCalledTimes(1);

    write.resolve(createSuccessfulPersistResult());

    await Promise.all([firstPersist, secondPersist]);

    expect(setCatalogSettingMock).toHaveBeenCalledTimes(1);
    expect(ctx.setState).toHaveBeenCalledTimes(1);
  });

  it('runs another persist pass when state changes and persist is requested during in-flight write', async () => {
    const firstWrite = createDeferred<PersistResult>();
    const secondWrite = createDeferred<PersistResult>();

    setCatalogSettingMock
      .mockReturnValueOnce(firstWrite.promise)
      .mockReturnValueOnce(secondWrite.promise);

    const persistence = createPersistence();
    const ctx = createPersistenceContext(createDirtyReadyState('before'));

    const firstPersist = persistence.persistFilters(ctx);

    ctx.updateState((current) => withSearchQuery(current, 'after'));

    const secondPersist = persistence.persistFilters(ctx);

    expect(setCatalogSettingMock).toHaveBeenCalledTimes(1);

    firstWrite.resolve(createSuccessfulPersistResult());
    await flushMicrotasks();

    expect(setCatalogSettingMock).toHaveBeenCalledTimes(2);

    secondWrite.resolve(createSuccessfulPersistResult());

    await Promise.all([firstPersist, secondPersist]);

    expect(ctx.setState).toHaveBeenCalledTimes(1);
  });

  it('does not commit stale snapshot when state changed during in-flight save', async () => {
    const write = createDeferred<PersistResult>();
    setCatalogSettingMock.mockReturnValueOnce(write.promise);

    const persistence = createPersistence();
    const ctx = createPersistenceContext(createDirtyReadyState('before'));

    const persist = persistence.persistFilters(ctx);

    ctx.updateState((current) => withSearchQuery(current, 'after'));

    write.resolve(createSuccessfulPersistResult());

    await persist;

    expect(setCatalogSettingMock).toHaveBeenCalledTimes(1);
    expect(ctx.setState).not.toHaveBeenCalled();
  });

  it('debounces search persistence and writes only latest state', async () => {
    vi.useFakeTimers();
    mockSuccessfulPersist();

    const persistence = createPersistence({ debounceMs: 100 });
    const ctx = createPersistenceContext(createDirtyReadyState('a'));

    persistence.queueSearchPersist(ctx);

    ctx.updateState((current) => withSearchQuery(current, 'ab'));
    persistence.queueSearchPersist(ctx);

    await vi.advanceTimersByTimeAsync(99);

    expect(setCatalogSettingMock).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);

    expect(setCatalogSettingMock).toHaveBeenCalledTimes(1);
    expectPersistedSearchQuery(0, 'ab');
  });

  it('flushes queued search persistence immediately', async () => {
    vi.useFakeTimers();
    mockSuccessfulPersist();

    const persistence = createPersistence({ debounceMs: 1000 });
    const ctx = createPersistenceContext(createDirtyReadyState('flush'));

    persistence.queueSearchPersist(ctx);
    persistence.flushQueuedSearchPersist(ctx);

    await flushMicrotasks();

    expect(setCatalogSettingMock).toHaveBeenCalledTimes(1);
    expectPersistedSearchQuery(0, 'flush');

    await vi.advanceTimersByTimeAsync(1000);

    expect(setCatalogSettingMock).toHaveBeenCalledTimes(1);
  });

  it('does not persist queued search when disposed before timer fires', async () => {
    vi.useFakeTimers();
    mockSuccessfulPersist();

    const persistence = createPersistence({ debounceMs: 200 });
    const ctx = createPersistenceContext(createDirtyReadyState('dispose'));

    persistence.queueSearchPersist(ctx);
    persistence.dispose();

    await vi.advanceTimersByTimeAsync(200);

    expect(setCatalogSettingMock).not.toHaveBeenCalled();
    expect(ctx.setState).not.toHaveBeenCalled();
  });

  it('does not commit finished in-flight persist after dispose', async () => {
    const write = createDeferred<PersistResult>();
    setCatalogSettingMock.mockReturnValueOnce(write.promise);

    const persistence = createPersistence();
    const ctx = createPersistenceContext(createDirtyReadyState('dispose'));

    const persist = persistence.persistFilters(ctx);

    persistence.dispose();
    write.resolve(createSuccessfulPersistResult());

    await persist;

    expect(setCatalogSettingMock).toHaveBeenCalledTimes(1);
    expect(ctx.setState).not.toHaveBeenCalled();
  });

  it('routes write errors to onPersistError callback', async () => {
    const error = new Error('write failed');
    const onPersistError = vi.fn();

    setCatalogSettingMock.mockRejectedValueOnce(error);

    const persistence = createPersistence({ onPersistError });
    const ctx = createPersistenceContext(createDirtyReadyState('failed'));

    await persistence.persistFilters(ctx);

    expect(onPersistError).toHaveBeenCalledTimes(1);
    expect(onPersistError).toHaveBeenCalledWith(error);
    expect(ctx.setState).not.toHaveBeenCalled();
  });

  it('continues working after failed persist', async () => {
    const error = new Error('write failed');
    const onPersistError = vi.fn();

    setCatalogSettingMock
      .mockRejectedValueOnce(error)
      .mockResolvedValueOnce(createSuccessfulPersistResult());

    const persistence = createPersistence({ onPersistError });
    const ctx = createPersistenceContext(createDirtyReadyState('failed'));

    await persistence.persistFilters(ctx);

    ctx.updateState((current) => withSearchQuery(current, 'recovered'));

    await persistence.persistFilters(ctx);

    expect(setCatalogSettingMock).toHaveBeenCalledTimes(2);

    expect(onPersistError).toHaveBeenCalledTimes(1);
    expect(ctx.setState).toHaveBeenCalledTimes(1);
  });

  it('does not reject when onPersistError throws', async () => {
    const writeError = new Error('write failed');
    const handlerError = new Error('handler failed');
    const onPersistError = vi.fn(() => {
      throw handlerError;
    });

    vi.spyOn(console, 'error').mockImplementation(() => undefined);

    setCatalogSettingMock.mockRejectedValueOnce(writeError);

    const persistence = createPersistence({ onPersistError });
    const ctx = createPersistenceContext(createDirtyReadyState('failed'));

    await expect(persistence.persistFilters(ctx)).resolves.toBeUndefined();

    expect(onPersistError).toHaveBeenCalledTimes(1);
    expect(onPersistError).toHaveBeenCalledWith(writeError);
    expect(ctx.setState).not.toHaveBeenCalled();
  });
});
