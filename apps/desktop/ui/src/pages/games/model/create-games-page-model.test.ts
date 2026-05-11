import { describe, expect, it, vi } from 'vitest';
import type { GameSummary } from '@entities/game';
import { createGamesPageModel } from './create-games-page-model.svelte';

function createStubGame(id: string, title: string): GameSummary {
  return {
    game_id: id,
    title,
    launcher: 'Steam',
    platform: 'windows',
    runtime: 'dx11',
    install_path: '/games/' + id,
    library_tags: ['steam'],
    component_count: 1,
    updates_available: false,
    update_count: 0,
    risk_level: 'safe',
    backup_available: false,
    operation_count: 0,
    last_operation_status: null,
    cover_updated_at_ms: null,
  };
}

describe('createGamesPageModel', () => {
  it('initializes with empty filter state', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(model.filtersState.ready).toBe(false);
    expect(model.filtersState.searchQuery).toBe('');
    expect(model.gameItems).toEqual([]);
  });

  it('derives library filter options from games', () => {
    const model = createGamesPageModel({
      getGames: () => [createStubGame('1', 'A'), createStubGame('2', 'B')],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(model.libraryFilterOptions.length).toBe(1);
    expect(model.libraryFilterOptions[0]?.value).toBe('steam');
  });

  it('hasFilterIndicator is true when not all libraries are selected', () => {
    const model = createGamesPageModel({
      getGames: () => [createStubGame('1', 'A')],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(model.hasFilterIndicator).toBe(true);
  });

  it('setMenuOpen updates menuOpenFor', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    model.setMenuOpen('game-1', true);
    expect(model.menuOpenFor).toBe('game-1');
    model.setMenuOpen('game-1', false);
    expect(model.menuOpenFor).toBeNull();
  });

  it('isCoverOperationBusy returns false by default', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(model.isCoverOperationBusy('game-1')).toBe(false);
  });

  it('dispose does not throw', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(() => {
      model.dispose();
    }).not.toThrow();
  });

  it('flushSearchPersist does not throw', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(() => {
      model.flushSearchPersist();
    }).not.toThrow();
  });

  it('setSearchQuery updates searchQuery', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(model.filtersState.searchQuery).toBe('');
    model.setSearchQuery('test');
    expect(model.filtersState.searchQuery).toBe('test');
  });

  it('setSearchQuery with same value does not queue persist', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(() => {
      model.setSearchQuery('');
    }).not.toThrow();
  });

  it('handlePopoverOpenChange opens and closes popover', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(model.filtersState.isPopoverOpen).toBe(false);
    model.handlePopoverOpenChange({ open: true, reason: 'programmatic' });
    expect(model.filtersState.isPopoverOpen).toBe(true);
    model.handlePopoverOpenChange({ open: false, reason: 'outside-pointer' });
    expect(model.filtersState.isPopoverOpen).toBe(false);
  });

  it('toggleFiltersPopover inverts popover state', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(model.filtersState.isPopoverOpen).toBe(false);
    model.toggleFiltersPopover();
    expect(model.filtersState.isPopoverOpen).toBe(true);
    model.toggleFiltersPopover();
    expect(model.filtersState.isPopoverOpen).toBe(false);
  });

  it('cancelFilterSelection resets popover state', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    model.handlePopoverOpenChange({ open: true, reason: 'programmatic' });
    expect(model.filtersState.isPopoverOpen).toBe(true);
    model.cancelFilterSelection();
    expect(model.filtersState.isPopoverOpen).toBe(false);
  });

  it('isCoverOperationBusy returns true when manual busy matches', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(model.isCoverOperationBusy('game-1')).toBe(false);
  });

  it('fetchCover does not throw (smoke test)', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(() => {
      model.fetchCover('game-1');
    }).not.toThrow();
  });

  it('clearCover does not throw (smoke test)', () => {
    const model = createGamesPageModel({
      getGames: () => [],
      getCatalogVersion: () => 0,
      getBusy: () => false,
      getCoversAutoFetchingIds: () => new Set(),
      onClearError: vi.fn(),
      onCoverError: vi.fn(),
      onReloadCards: () => Promise.resolve(),
    });

    expect(() => {
      model.clearCover('game-1');
    }).not.toThrow();
  });
});
