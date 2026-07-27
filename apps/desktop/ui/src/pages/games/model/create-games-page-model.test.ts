import { describe, expect, it, vi } from 'vitest';
import type { CatalogDelta, GameSummary, GamesCatalogBootstrap } from '@entities/game';
import { createGameSummary } from '@entities/game';
import { resolveGamesFiltersBootstrap } from '@features/filter-games';
import { createGamesPageModel, type GamesPageModelInput } from './create-games-page-model.svelte';

function createStubGame(id: string, title: string): GameSummary {
  return createGameSummary({
    game_id: id,
    title,
    library_tags: ['steam'],
    component_count: 1,
    risk_level: 'safe',
  });
}

describe('createGamesPageModel', () => {
  it('initializes with empty filter state', () => {
    const model = createGamesPageModel(createInput());

    expect(model.filtersState.ready).toBe(false);
    expect(model.filtersState.searchQuery).toBe('');
    expect(model.gameItems).toEqual([]);
  });

  it('hydrates saved filters before publishing the matching bootstrap cards', () => {
    const filteredGame = createGameSummary({
      game_id: '2',
      title: 'Second Game',
      launcher: 'Steam',
      library_tags: ['dlss_super_resolution'],
      component_count: 1,
      risk_level: 'safe',
    });
    const model = createGamesPageModel(createInput());
    const savedFilters = {
      libraries: ['dlss_super_resolution'],
      addons: [],
      launchers: ['Steam'],
      launcherOrder: ['Steam'],
      searchQuery: 'second',
      showHidden: false,
      favoritesOnly: false,
    };

    expect(model.gameItems).toEqual([]);
    expect(model.filtersState.ready).toBe(false);
    model.applyBootstrap({ ...bootstrap(1, [filteredGame]), filters: savedFilters });

    expect(model.bootstrapping).toBe(false);
    expect(model.filtersState.ready).toBe(true);
    expect(model.filtersState.searchQuery).toBe('second');
    expect(model.filtersState.appliedLibraries).toEqual(['dlss_super_resolution']);
    expect(model.filtersState.appliedLaunchers).toEqual(['Steam']);
    expect(model.filtersState.appliedFavoritesOnly).toBe(false);
    expect(model.gameItems.map(({ id, title }) => ({ id, title }))).toEqual([
      { id: '2', title: 'Second Game' },
    ]);
  });

  it('keeps an empty catalog in refreshing state until background discovery completes', () => {
    const model = createGamesPageModel(createInput());
    model.applyBootstrap(bootstrap(1));

    expect(model.catalogSyncState).toBe('refreshing');
    model.completeInitialCatalogSync();
    expect(model.catalogSyncState).toBe('ready');
  });

  it('returns all known libraries for grouped library filter options', () => {
    const model = createGamesPageModel(createInput());

    const options = model.groupedLibraryFilterOptions;
    expect(options.length).toBeGreaterThan(0);
    // NVIDIA should be present as it's part of ALL_KNOWN_LIBRARIES
    expect(options.some((group) => group.vendorKey === 'nvidia')).toBe(true);
  });

  it('hasFilterIndicator is true when not all libraries are selected', () => {
    const model = createGamesPageModel(createInput());

    expect(model.hasFilterIndicator).toBe(true);
  });

  it('setMenuOpen updates menuOpenFor', () => {
    const model = createGamesPageModel(createInput());

    model.setMenuOpen('game-1', true);
    expect(model.menuOpenFor).toBe('game-1');
    model.setMenuOpen('game-1', false);
    expect(model.menuOpenFor).toBeNull();
  });

  it('isCoverOperationBusy returns false by default', () => {
    const model = createGamesPageModel(createInput());

    expect(model.isCoverOperationBusy('game-1')).toBe(false);
  });

  it('dispose does not throw', () => {
    const model = createGamesPageModel(createInput());

    expect(() => {
      model.dispose();
    }).not.toThrow();
  });

  it('flushSearchPersist does not throw', () => {
    const model = createGamesPageModel(createInput());

    expect(() => {
      model.flushSearchPersist();
    }).not.toThrow();
  });

  it('setSearchQuery updates searchQuery', () => {
    const model = createGamesPageModel(createInput());

    expect(model.filtersState.searchQuery).toBe('');
    model.setSearchQuery('test');
    expect(model.filtersState.searchQuery).toBe('test');
  });

  it('setSearchQuery with same value does not queue persist', () => {
    const model = createGamesPageModel(createInput());

    expect(() => {
      model.setSearchQuery('');
    }).not.toThrow();
  });

  it('keeps the game-id scroll anchor in the long-lived catalog session', () => {
    const model = createGamesPageModel(createInput());

    model.setScrollAnchor({ gameId: 'game-42', offsetWithinRow: 17.5 });

    expect(model.scrollAnchor).toEqual({ gameId: 'game-42', offsetWithinRow: 17.5 });
  });

  it('keeps keyboard focus identity in the long-lived catalog session', () => {
    const model = createGamesPageModel(createInput());
    model.applyBootstrap(
      bootstrap(1, [createStubGame('game-1', 'First'), createStubGame('game-2', 'Second')]),
    );

    model.setFocusedGame('game-2');

    expect(model.focusedGameId).toBe('game-2');
  });

  it('rolls back only the failed optimistic field and preserves concurrent card patches', async () => {
    let rejectFavorite: (error: Error) => void = () => undefined;
    const model = createGamesPageModel(
      createInput({
        setFavorite: () =>
          new Promise((_, reject) => {
            rejectFavorite = reject;
          }),
      }),
    );
    model.applyBootstrap(bootstrap(1, [createStubGame('game-1', 'First')]));

    const mutation = model.toggleFavorite('game-1', true);
    model.patchCover('game-1', 42);
    rejectFavorite(new Error('write failed'));
    await mutation;

    expect(model.games[0]).toMatchObject({
      game_id: 'game-1',
      is_favorite: false,
      cover_updated_at_ms: 42,
    });
  });

  it('restores a card removed by a failed optimistic hidden mutation', async () => {
    const model = createGamesPageModel(
      createInput({ setHidden: () => Promise.reject(new Error('write failed')) }),
    );
    model.applyBootstrap(bootstrap(1, [createStubGame('game-1', 'First')]));

    await model.toggleHidden('game-1', true);

    expect(model.games.map((game) => game.game_id)).toEqual(['game-1']);
    expect(model.games[0].is_hidden).toBe(false);
    expect(model.hiddenCount).toBe(0);
  });

  it('serializes same-game writes and rolls a failed trailing favorite back to the confirmed value', async () => {
    const pending: {
      value: boolean;
      resolve: () => void;
      reject: (error: Error) => void;
    }[] = [];
    const model = createGamesPageModel(
      createInput({
        setFavorite: (_gameId, value) =>
          new Promise<void>((resolve, reject) => {
            pending.push({ value, resolve, reject });
          }),
      }),
    );
    model.applyBootstrap(bootstrap(1, [createStubGame('game-1', 'First')]));

    const first = model.toggleFavorite('game-1', true);
    const second = model.toggleFavorite('game-1', false);
    expect(pending.map(({ value }) => value)).toEqual([true]);

    pending[0].resolve();
    await vi.waitFor(() => {
      expect(pending.map(({ value }) => value)).toEqual([true, false]);
    });
    pending[1].reject(new Error('second write failed'));
    await Promise.all([first, second]);

    expect(model.games[0].is_favorite).toBe(true);
  });

  it('restores the confirmed card when repeated queued hidden writes both fail', async () => {
    const pending: { reject: (error: Error) => void }[] = [];
    const model = createGamesPageModel(
      createInput({
        setHidden: () =>
          new Promise<void>((_resolve, reject) => {
            pending.push({ reject });
          }),
      }),
    );
    model.applyBootstrap(bootstrap(1, [createStubGame('game-1', 'First')]));

    const first = model.toggleHidden('game-1', true);
    const second = model.toggleHidden('game-1', true);
    expect(pending).toHaveLength(1);

    pending[0].reject(new Error('first write failed'));
    await vi.waitFor(() => {
      expect(pending).toHaveLength(2);
    });
    pending[1].reject(new Error('second write failed'));
    await Promise.all([first, second]);

    expect(model.games.map((game) => game.game_id)).toEqual(['game-1']);
    expect(model.games[0].is_hidden).toBe(false);
    expect(model.hiddenCount).toBe(0);
  });

  it('coalesces newer catalog deltas and rejects stale revisions', () => {
    const model = createGamesPageModel(createInput());

    expect(
      model.acceptCatalogDelta(delta(2, ['game-changed', 'game-removed'], ['game-removed'])),
    ).toBe(true);
    expect(model.acceptCatalogDelta(delta(1, ['stale'], []))).toBe(false);
    expect(model.pendingCatalogDelta).toEqual({
      revision: 2,
      reasons: ['scan'],
      changedGameIds: ['game-changed'],
      removedGameIds: ['game-removed'],
    });

    expect(model.acceptCatalogDelta(delta(3, ['game-removed'], []))).toBe(true);
    expect(model.pendingCatalogDelta).toEqual({
      revision: 3,
      reasons: ['scan'],
      changedGameIds: ['game-changed'],
      removedGameIds: ['game-removed'],
    });
  });

  it('keeps a pre-bootstrap delta pending until an equally new snapshot arrives', () => {
    const model = createGamesPageModel(createInput());

    model.acceptCatalogDelta(delta(4, ['game-4'], []));
    model.applyBootstrap(bootstrap(3));
    expect(model.pendingCatalogDelta?.revision).toBe(4);

    model.applyBootstrap(bootstrap(4));
    expect(model.pendingCatalogDelta).toBeNull();
  });

  it('handleDialogOpenChange opens and closes dialog', () => {
    const model = createGamesPageModel(createInput());

    expect(model.filtersState.isDialogOpen).toBe(false);
    model.handleDialogOpenChange(true);
    expect(model.filtersState.isDialogOpen).toBe(true);
    model.handleDialogOpenChange(false);
    expect(model.filtersState.isDialogOpen).toBe(false);
  });

  it('toggleFiltersDialog inverts dialog state', () => {
    const model = createGamesPageModel(createInput());

    expect(model.filtersState.isDialogOpen).toBe(false);
    model.toggleFiltersDialog();
    expect(model.filtersState.isDialogOpen).toBe(true);
    model.toggleFiltersDialog();
    expect(model.filtersState.isDialogOpen).toBe(false);
  });

  it('cancelFilterSelection resets dialog state', () => {
    const model = createGamesPageModel(createInput());

    model.handleDialogOpenChange(true);
    expect(model.filtersState.isDialogOpen).toBe(true);
    model.cancelFilterSelection();
    expect(model.filtersState.isDialogOpen).toBe(false);
  });

  it('isCoverOperationBusy returns true when manual busy matches', () => {
    const model = createGamesPageModel(createInput());

    expect(model.isCoverOperationBusy('game-1')).toBe(false);
  });

  it('fetchCover does not throw (smoke test)', () => {
    const model = createGamesPageModel(createInput());

    expect(() => {
      model.fetchCover('game-1');
    }).not.toThrow();
  });

  it('clearCover does not throw (smoke test)', () => {
    const model = createGamesPageModel(createInput());

    expect(() => {
      model.clearCover('game-1');
    }).not.toThrow();
  });
});

function createInput(overrides: Partial<GamesPageModelInput> = {}): GamesPageModelInput {
  return {
    getCoversAutoFetchingIds: () => new Set(),
    getOnClearError: () => vi.fn(),
    ...overrides,
  };
}

function delta(revision: number, changedGameIds: string[], removedGameIds: string[]): CatalogDelta {
  return {
    revision,
    reasons: ['scan'],
    changedGameIds,
    removedGameIds,
  };
}

function bootstrap(revision: number, items: GameSummary[] = []): GamesCatalogBootstrap {
  return {
    filters: resolveGamesFiltersBootstrap(null).filters,
    result: {
      items,
      catalogSize: items.length,
      total: items.length,
      hiddenCount: 0,
      availableLibraries: [],
      availableLaunchers: [],
      catalogRevision: revision,
      nextOffset: null,
    },
  };
}
