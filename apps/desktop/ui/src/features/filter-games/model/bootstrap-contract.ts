import {
  ALL_ADDON_CAPABILITIES,
  ALL_KNOWN_LAUNCHERS,
  expandLibraryFilterAliases,
  normalizeAddonCapabilities,
  type AddonCapability,
  type EffectiveGamesFilters,
} from '@entities/game';
import { ALL_KNOWN_LIBRARIES } from '@shared/graphics';
import { parsePersistedGamesFilters } from './filter-persistence';
import { createInitialGamesFilterState, hydrateGamesFilterState } from './games-filter-state';

export type GamesFiltersBootstrapContract = {
  filters: EffectiveGamesFilters;
  searchQuery: string;
  selectedLibraries: string[];
  selectedAddons: AddonCapability[];
  selectedLaunchers: string[];
  launcherOrder: string[];
  showHidden: boolean;
  favoritesOnly: boolean;
};

/**
 * Produces the normalized filter state used by frontend-backed desktop mocks.
 * The real backend returns the same effective shape from `bootstrap_games_catalog`.
 */
export function resolveGamesFiltersBootstrap(
  persistedValue: string | null,
): GamesFiltersBootstrapContract {
  const state = hydrateGamesFilterState(
    createInitialGamesFilterState(),
    parsePersistedGamesFilters(persistedValue),
    ALL_KNOWN_LIBRARIES,
    ALL_KNOWN_LAUNCHERS,
    ALL_ADDON_CAPABILITIES,
  );
  const filters: EffectiveGamesFilters = {
    libraries: [...state.appliedLibraries],
    addons: normalizeAddonCapabilities(state.appliedAddons),
    launchers: [...state.appliedLaunchers],
    launcherOrder: [...state.appliedLauncherOrder],
    searchQuery: state.searchQuery,
    showHidden: state.appliedShowHidden,
    favoritesOnly: state.appliedFavoritesOnly,
  };

  return {
    filters,
    searchQuery: state.searchQuery,
    selectedLibraries: expandLibraryFilterAliases(state.appliedLibraries),
    selectedAddons: normalizeAddonCapabilities(state.appliedAddons),
    selectedLaunchers: state.appliedLaunchers,
    launcherOrder: state.appliedLauncherOrder,
    showHidden: state.appliedShowHidden,
    favoritesOnly: state.appliedFavoritesOnly,
  };
}
