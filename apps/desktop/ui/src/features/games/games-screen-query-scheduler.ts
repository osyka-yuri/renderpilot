import { queryGameCards } from '@shared/api/desktop';
import {
  DEFAULT_GAME_CARDS_CATALOG_PAGE,
  DEFAULT_GAME_CARDS_CATALOG_SORT,
} from '@shared/api/game-cards-defaults';
import type { GameCard } from '@shared/api/types';
import { createRequestChannel, type RequestChannel } from '@shared/utils/request-channel';

import { buildGameCardsQueryKey, shallowStringArrayEqual } from './games-screen-filters';

export type GamesQuerySnapshot = {
  requestKey: string;
  searchQuery: string;
  selectedLibraries: string[];
};

export type GamesQueryResultSinks = {
  setItems(items: GameCard[]): void;
  getLibraries(): string[];
  setLibraries(libraries: string[]): void;
};

type SchedulerOptions = {
  queryGameCardsFn?: typeof queryGameCards;
  requests?: RequestChannel;
};

const EMPTY_REQUEST_KEY = '';

function createRequestKey(
  version: number,
  searchQuery: string,
  selectedLibraries: readonly string[],
) {
  return `${version}:${buildGameCardsQueryKey(searchQuery, selectedLibraries)}`;
}

function updateAvailableLibraries(
  sinks: GamesQueryResultSinks,
  availableLibraries: readonly string[],
): void {
  const currentLibraries = sinks.getLibraries();

  if (shallowStringArrayEqual(currentLibraries, availableLibraries)) {
    return;
  }

  sinks.setLibraries([...availableLibraries]);
}

export function createGamesScreenQueryScheduler(options: SchedulerOptions = {}) {
  const fetchCards = options.queryGameCardsFn ?? queryGameCards;
  const requests = options.requests ?? createRequestChannel();

  let lastHandledRequestKey = EMPTY_REQUEST_KEY;
  let activeRequestKey = EMPTY_REQUEST_KEY;

  function createGamesQuerySnapshot(
    version: number,
    filtersReady: boolean,
    preferenceLoaded: boolean,
    searchQuery: string,
    selectedLibraries: readonly string[],
  ): GamesQuerySnapshot | null {
    if (!filtersReady || !preferenceLoaded) {
      return null;
    }

    const normalizedSearchQuery = searchQuery.trim();
    const normalizedSelectedLibraries = [...selectedLibraries];

    return {
      requestKey: createRequestKey(version, normalizedSearchQuery, normalizedSelectedLibraries),
      searchQuery: normalizedSearchQuery,
      selectedLibraries: normalizedSelectedLibraries,
    };
  }

  function canRunGamesQuery(requestKey: string): boolean {
    return requestKey !== lastHandledRequestKey && requestKey !== activeRequestKey;
  }

  async function runGamesQuery(
    snapshot: GamesQuerySnapshot,
    sinks: GamesQueryResultSinks,
  ): Promise<void> {
    if (!canRunGamesQuery(snapshot.requestKey)) {
      return;
    }

    const requestId = requests.begin();

    activeRequestKey = snapshot.requestKey;

    try {
      const result = await fetchCards({
        searchQuery: snapshot.searchQuery,
        selectedLibraries: snapshot.selectedLibraries,
        sort: DEFAULT_GAME_CARDS_CATALOG_SORT,
        page: DEFAULT_GAME_CARDS_CATALOG_PAGE,
      });

      if (!requests.isActive(requestId)) {
        return;
      }

      sinks.setItems(result.items);
      updateAvailableLibraries(sinks, result.availableLibraries);
    } catch (error: unknown) {
      if (requests.isActive(requestId)) {
        console.error('Failed to query game cards.', error);
      }
    } finally {
      const isCurrentRequest = requests.isActive(requestId);

      if (isCurrentRequest) {
        lastHandledRequestKey = snapshot.requestKey;
      }

      if (activeRequestKey === snapshot.requestKey) {
        activeRequestKey = EMPTY_REQUEST_KEY;
      }
    }
  }

  return {
    createGamesQuerySnapshot,
    canRunGamesQuery,
    runGamesQuery,
  };
}
