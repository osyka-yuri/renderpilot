import type { GameCardsQuery, GameCardsResult, GameDetails } from '@entities/game';
import { normalizeGameCardsQuery } from '@entities/game';
import { mockState, requireGameDetails } from '../desktop-state';
import {
  clone,
  collectAvailableLibraries,
  collectAvailableLaunchers,
  requireNonEmptyText,
  resolveMock,
} from '../desktop-utils';
import { buildGameCardFilterContext, matchesGameCardFilters, sortGameCards } from './query-filters';

export function mockQueryGameCards(query: GameCardsQuery): Promise<GameCardsResult> {
  return resolveMock(() => {
    const normalizedQuery = normalizeGameCardsQuery(query);
    const allCards = clone(mockState.games);

    const availableLibraries = collectAvailableLibraries(allCards);
    const availableLibrarySet = new Set(availableLibraries);
    const availableLaunchers = collectAvailableLaunchers(allCards);
    const availableLauncherSet = new Set(availableLaunchers);

    // Drop selections the catalog no longer offers (stale filter persistence).
    const effectiveQuery = {
      ...normalizedQuery,
      selectedLibraries: normalizedQuery.selectedLibraries.filter((library) =>
        availableLibrarySet.has(library),
      ),
      selectedLaunchers: normalizedQuery.selectedLaunchers.filter((launcher) =>
        availableLauncherSet.has(launcher),
      ),
    };

    const filterContext = buildGameCardFilterContext(effectiveQuery);
    const filtered = sortGameCards(
      allCards.filter((card) => matchesGameCardFilters(card, filterContext)),
      effectiveQuery.sort,
    );

    const hiddenCount = allCards.filter((card) => card.is_hidden).length;
    const offset = effectiveQuery.page.offset;
    const limit = effectiveQuery.page.limit;

    return {
      items: filtered.slice(offset, offset + limit),
      total: filtered.length,
      hiddenCount,
      availableLibraries,
      availableLaunchers,
      queryFingerprint: JSON.stringify(normalizedQuery),
    };
  });
}

export function mockGetGameDetails(gameId: string): Promise<GameDetails> {
  return resolveMock(() => clone(requireGameDetails(requireNonEmptyText(gameId, 'game id'))));
}
