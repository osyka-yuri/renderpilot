import { describe, expect, it } from 'vitest';
import {
  createInitialGamesFilterState,
  hydrateGamesFilterState,
  withAvailableCatalogFilters,
} from './games-filter-state';
import type { PersistedGamesFilters } from './filter-persistence';

type AvailableLibraries = readonly string[];
type GamesFilterState = ReturnType<typeof hydrateGamesFilterState>;

const regressionLibraryCatalog = ['LibraryAlpha', 'LibraryBeta'] as const;

const createAvailableLibraries = (): AvailableLibraries => [...regressionLibraryCatalog];

const createEmptyLibraries = (): AvailableLibraries => [];

const createPersistedFilters = (
  overrides: Partial<PersistedGamesFilters> = {},
): PersistedGamesFilters => ({
  libraries: [],
  launchers: [],
  launcherOrder: [],
  searchQuery: '',
  ...overrides,
});

const hydrateFilters = (
  persistedFilters: PersistedGamesFilters | null,
  availableLibraries: AvailableLibraries = createAvailableLibraries(),
): GamesFilterState =>
  hydrateGamesFilterState(
    createInitialGamesFilterState(),
    persistedFilters,
    availableLibraries,
    [],
  );

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
    const afterCatalogLoaded = withAvailableCatalogFilters(
      firstMount,
      createAvailableLibraries(),
      [],
    );

    expectSelectedLibraries(afterCatalogLoaded, createAvailableLibraries());
  });

  it('does not hide games when catalog temporarily becomes unknown', () => {
    const hydrated = hydrateFilters(null);
    const afterCatalogLost = withAvailableCatalogFilters(hydrated, createEmptyLibraries(), []);

    expectSelectedLibraries(afterCatalogLost, createEmptyLibraries());
  });

  it('applies persisted library selection when catalog becomes known after first hydrate', () => {
    const persistedFilters = createPersistedFilters({
      libraries: ['LibraryAlpha'],
    });

    const firstHydrate = hydrateFilters(persistedFilters, createEmptyLibraries());

    expect(firstHydrate.appliedLibraries).toEqual([]);

    const afterCatalogLoaded = withAvailableCatalogFilters(
      firstHydrate,
      createAvailableLibraries(),
      [],
    );

    expectSelectedLibraries(afterCatalogLoaded, ['LibraryAlpha']);
  });
});
