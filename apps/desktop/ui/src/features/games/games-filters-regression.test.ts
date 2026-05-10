import { describe, expect, it } from 'vitest';
import {
  createInitialGamesFilterState,
  hydrateGamesFilterState,
  withAvailableLibraries,
} from '@features/games/games-filter-state';
import type { PersistedGamesFilters } from '@features/games/games-screen-filters';
import { regressionLibraryCatalog } from '@features/games/games-test-fixtures';

type AvailableLibraries = Parameters<typeof hydrateGamesFilterState>[2];
type GamesFilterState = ReturnType<typeof hydrateGamesFilterState>;

const createAvailableLibraries = (): AvailableLibraries => [...regressionLibraryCatalog];

const createEmptyLibraries = (): AvailableLibraries => [];

const createPersistedFilters = (
  overrides: Partial<PersistedGamesFilters> = {},
): PersistedGamesFilters => ({
  libraries: [],
  searchQuery: '',
  ...overrides,
});

const hydrateFilters = (
  persistedFilters: PersistedGamesFilters | null,
  availableLibraries: AvailableLibraries = createAvailableLibraries(),
): GamesFilterState =>
  hydrateGamesFilterState(createInitialGamesFilterState(), persistedFilters, availableLibraries);

const expectSelectedLibraries = (
  state: GamesFilterState,
  expectedLibraries: AvailableLibraries,
): void => {
  expect(state.appliedLibraries).toEqual(expectedLibraries);
  expect(state.draftLibraries).toEqual(expectedLibraries);
};

describe('games filters regressions', () => {
  it('keeps visual and factual state in sync after settings back navigation', () => {
    const persistedFilters = createPersistedFilters({
      libraries: ['LibraryAlpha'],
      searchQuery: 'war',
    });

    const beforeSettingsNavigation = hydrateFilters(persistedFilters);
    const afterSettingsBackNavigation = hydrateFilters(persistedFilters);

    expect(afterSettingsBackNavigation.appliedLibraries).toEqual(
      beforeSettingsNavigation.appliedLibraries,
    );
    expect(afterSettingsBackNavigation.searchQuery).toBe(beforeSettingsNavigation.searchQuery);
  });

  it('does not produce phantom filtering when libraries appear later', () => {
    const firstMount = hydrateFilters(null, createEmptyLibraries());
    const afterCatalogLoaded = withAvailableLibraries(firstMount, createAvailableLibraries());

    expectSelectedLibraries(afterCatalogLoaded, createAvailableLibraries());
  });

  it('does not hide games when catalog temporarily becomes unknown', () => {
    const hydrated = hydrateFilters(null);
    const afterCatalogLost = withAvailableLibraries(hydrated, createEmptyLibraries());

    expectSelectedLibraries(afterCatalogLost, createEmptyLibraries());
  });

  it('applies persisted library selection when catalog becomes known after first hydrate', () => {
    const persistedFilters = createPersistedFilters({
      libraries: ['LibraryAlpha'],
    });

    const firstHydrate = hydrateFilters(persistedFilters, createEmptyLibraries());

    expect(firstHydrate.appliedLibraries).toEqual([]);

    const afterCatalogLoaded = withAvailableLibraries(firstHydrate, createAvailableLibraries());

    expectSelectedLibraries(afterCatalogLoaded, ['LibraryAlpha']);
  });
});
