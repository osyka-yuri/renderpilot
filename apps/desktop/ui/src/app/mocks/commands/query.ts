import type { GameCardsQuery, GameCardsResult, GameDetails } from '@entities/game';
import { normalizeGameCardsQuery } from '@entities/game';
import { mockState, requireGameDetails } from '../desktop-state';
import {
  clone,
  collectAvailableLibraries,
  collectAvailableLaunchers,
  compareCards,
  requireNonEmptyText,
  resolveMock,
} from '../desktop-utils';

export function mockQueryGameCards(query: GameCardsQuery): Promise<GameCardsResult> {
  return resolveMock(() => {
    const normalizedQuery = normalizeGameCardsQuery(query);
    const allCards = clone(mockState.games);

    const availableLibraries = collectAvailableLibraries(allCards);
    const availableLibrarySet = new Set(availableLibraries);
    const selectedLibrarySet = new Set(
      normalizedQuery.selectedLibraries.filter((library) => availableLibrarySet.has(library)),
    );
    const hasLibraryFilter = normalizedQuery.selectedLibraries.length > 0;

    const availableLaunchers = collectAvailableLaunchers(allCards);
    const availableLauncherSet = new Set(availableLaunchers);
    const selectedLauncherSet = new Set(
      normalizedQuery.selectedLaunchers.filter((launcher) => availableLauncherSet.has(launcher)),
    );
    const hasLauncherFilter = normalizedQuery.selectedLaunchers.length > 0;

    const searchQuery = normalizedQuery.searchQuery.trim().toLowerCase();

    const filtered = allCards
      .filter((card) => {
        if (card.is_hidden && !normalizedQuery.showHidden) {
          return false;
        }

        if (normalizedQuery.favoritesOnly && !card.is_favorite) {
          return false;
        }

        const matchesSearch =
          searchQuery.length === 0 || card.title.toLowerCase().includes(searchQuery);

        const matchesLibraries =
          !hasLibraryFilter || card.library_tags.some((library) => selectedLibrarySet.has(library));

        const matchesLaunchers = !hasLauncherFilter || selectedLauncherSet.has(card.launcher);

        return matchesSearch && matchesLibraries && matchesLaunchers;
      })
      .sort((left, right) => {
        // Favorites always float to the top, mirroring the Rust backend behaviour.
        const favoriteDiff = Number(right.is_favorite) - Number(left.is_favorite);
        if (favoriteDiff !== 0) return favoriteDiff;
        return compareCards(left, right, normalizedQuery.sort);
      });

    const hiddenCount = allCards.filter((card) => card.is_hidden).length;

    const offset = normalizedQuery.page.offset;
    const limit = normalizedQuery.page.limit;

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
