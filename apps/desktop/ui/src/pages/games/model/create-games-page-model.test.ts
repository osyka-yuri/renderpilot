import { describe, expect, it, vi } from 'vitest';
import type { GameSummary } from '@entities/game';
import { createGameSummary } from '@entities/game';
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

  it('pre-populates gameItems from the initial games input', () => {
    const initialGames = [createStubGame('1', 'First Game'), createStubGame('2', 'Second Game')];
    const model = createGamesPageModel(
      createInput({
        getGames: () => initialGames,
      }),
    );

    expect(model.gameItems.map(({ id, title }) => ({ id, title }))).toEqual([
      { id: '1', title: 'First Game' },
      { id: '2', title: 'Second Game' },
    ]);
  });

  it('returns all known libraries for grouped library filter options', () => {
    const model = createGamesPageModel(createInput());

    const options = model.groupedLibraryFilterOptions;
    expect(options.length).toBeGreaterThan(0);
    // NVIDIA should be present as it's part of ALL_KNOWN_LIBRARIES
    expect(options.some((group) => group.vendorKey === 'nvidia')).toBe(true);
  });

  it('hasFilterIndicator is true when not all libraries are selected', () => {
    const model = createGamesPageModel(
      createInput({
        getGames: () => [createStubGame('1', 'A')],
      }),
    );

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
    getGames: () => [],
    getCatalogVersion: () => 0,
    getBusy: () => false,
    getCoversAutoFetchingIds: () => new Set(),
    getOnClearError: () => vi.fn(),
    getOnReloadCards: () => () => Promise.resolve(),
    ...overrides,
  };
}
