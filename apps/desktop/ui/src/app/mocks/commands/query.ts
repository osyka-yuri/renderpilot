import type { GameCardsQuery, GameCardsResult, GameDetails } from '@entities/game';
import { normalizeGameCardsQuery } from '@entities/game';
import { mockState, requireGameDetails } from '../desktop-state';
import {
  clone,
  collectAvailableLibraries,
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

    const searchQuery = normalizedQuery.searchQuery.trim().toLowerCase();

    const filtered = allCards
      .filter((card) => {
        const matchesSearch =
          searchQuery.length === 0 || card.title.toLowerCase().includes(searchQuery);

        const matchesLibraries =
          selectedLibrarySet.size === 0 ||
          card.library_tags.some((library) => selectedLibrarySet.has(library));

        return matchesSearch && matchesLibraries;
      })
      .sort((left, right) => compareCards(left, right, normalizedQuery.sort));

    const offset = normalizedQuery.page.offset;
    const limit = normalizedQuery.page.limit;

    return {
      items: filtered.slice(offset, offset + limit),
      total: filtered.length,
      availableLibraries,
      queryFingerprint: JSON.stringify(normalizedQuery),
    };
  });
}

export function mockGetGameDetails(gameId: string): Promise<GameDetails> {
  return resolveMock(() => clone(requireGameDetails(requireNonEmptyText(gameId, 'game id'))));
}
