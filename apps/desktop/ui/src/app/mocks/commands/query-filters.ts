import type { GameCardsQuery, GameSummary } from '@entities/game';
import { normalizeAddonCapabilities } from '@entities/game';

import { compareCards } from '../desktop-utils';

export type GameCardFilterContext = {
  searchQuery: string;
  selectedLibrarySet: Set<string>;
  hasLibraryFilter: boolean;
  selectedAddonSet: Set<string>;
  hasAddonFilter: boolean;
  selectedLauncherSet: Set<string>;
  hasLauncherFilter: boolean;
  showHidden: boolean;
  favoritesOnly: boolean;
};

/** Builds the filter context once per query so the card loop stays allocation-light. */
export function buildGameCardFilterContext(query: GameCardsQuery): GameCardFilterContext {
  // Mirror backend normalize_addon_names: drop unknown kinds so an all-unknown
  // selection becomes "no addon filter" instead of matching nothing.
  const selectedAddons = normalizeAddonCapabilities(query.selectedAddons);
  return {
    searchQuery: query.searchQuery.trim().toLowerCase(),
    selectedLibrarySet: new Set(query.selectedLibraries),
    hasLibraryFilter: query.selectedLibraries.length > 0,
    selectedAddonSet: new Set(selectedAddons),
    hasAddonFilter: selectedAddons.length > 0,
    selectedLauncherSet: new Set(query.selectedLaunchers),
    hasLauncherFilter: query.selectedLaunchers.length > 0,
    showHidden: query.showHidden,
    favoritesOnly: query.favoritesOnly,
  };
}

/**
 * Pure card predicate used by the mock query path. Library + addon filters OR when
 * both are present (mirrors backend catalog query semantics).
 */
export function matchesGameCardFilters(card: GameSummary, ctx: GameCardFilterContext): boolean {
  if (card.is_hidden && !ctx.showHidden) {
    return false;
  }

  if (ctx.favoritesOnly && !card.is_favorite) {
    return false;
  }

  const matchesSearch =
    ctx.searchQuery.length === 0 || card.title.toLowerCase().includes(ctx.searchQuery);

  if (!matchesSearch) {
    return false;
  }

  if (ctx.hasLauncherFilter && !ctx.selectedLauncherSet.has(card.launcher)) {
    return false;
  }

  const matchesLibraries =
    !ctx.hasLibraryFilter ||
    card.library_tags.some((library) => ctx.selectedLibrarySet.has(library));
  const matchesAddons =
    !ctx.hasAddonFilter || card.addon_capabilities.some((addon) => ctx.selectedAddonSet.has(addon));

  if (ctx.hasLibraryFilter && ctx.hasAddonFilter) {
    return matchesLibraries || matchesAddons;
  }
  if (ctx.hasLibraryFilter) {
    return matchesLibraries;
  }
  if (ctx.hasAddonFilter) {
    return matchesAddons;
  }
  return true;
}

/** Favorites first, then the requested sort field (title / updates / risk). */
export function sortGameCards(
  cards: readonly GameSummary[],
  sort: GameCardsQuery['sort'],
): GameSummary[] {
  return [...cards].sort((left, right) => {
    const favoriteDiff = Number(right.is_favorite) - Number(left.is_favorite);
    if (favoriteDiff !== 0) {
      return favoriteDiff;
    }
    return compareCards(left, right, sort);
  });
}
