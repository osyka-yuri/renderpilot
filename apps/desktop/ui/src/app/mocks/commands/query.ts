import type { GameCardsResult, GameDetails } from '@entities/game';
import {
  ALL_KNOWN_LAUNCHERS,
  expandLibraryFilterAliases,
  normalizeGameCardsQuery,
} from '@entities/game';
import { ALL_KNOWN_LIBRARIES } from '@shared/graphics';
import { mockState, requireGameDetails } from '../desktop-state';
import {
  clone,
  collectAvailableLibraries,
  collectAvailableLaunchers,
  requireNonEmptyText,
  resolveMock,
} from '../desktop-utils';
import { buildGameCardFilterContext, matchesGameCardFilters, sortGameCards } from './query-filters';

const KNOWN_QUERY_LIBRARIES = new Set(expandLibraryFilterAliases(ALL_KNOWN_LIBRARIES));
const KNOWN_QUERY_LAUNCHERS = new Set<string>(ALL_KNOWN_LAUNCHERS);

export function mockQueryGameCards(query: unknown): Promise<GameCardsResult> {
  return resolveMock(() => {
    const normalizedQuery = normalizeGameCardsQuery(query);
    const allCards = clone(mockState.games);

    const availableLibraries = collectAvailableLibraries(allCards);
    const availableLaunchers = collectAvailableLaunchers(allCards);

    // Match the backend contract: reject values outside the domain vocabulary,
    // but keep known selections active even when the current catalog has no match.
    const effectiveQuery = {
      ...normalizedQuery,
      selectedLibraries: normalizedQuery.selectedLibraries.filter((library) =>
        KNOWN_QUERY_LIBRARIES.has(library),
      ),
      selectedLaunchers: normalizedQuery.selectedLaunchers.filter((launcher) =>
        KNOWN_QUERY_LAUNCHERS.has(launcher),
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
      catalogSize: allCards.length,
      total: filtered.length,
      hiddenCount,
      availableLibraries,
      availableLaunchers,
      catalogRevision: 1,
      nextOffset: offset + limit < filtered.length ? offset + limit : null,
    };
  });
}

export function mockGetGameDetails(gameId: string): Promise<GameDetails> {
  return resolveMock(() => clone(requireGameDetails(requireNonEmptyText(gameId, 'game id'))));
}
