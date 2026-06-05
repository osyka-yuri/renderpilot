import {
  queryGameCards,
  DEFAULT_GAME_CARDS_CATALOG_PAGE,
  DEFAULT_GAME_CARDS_CATALOG_SORT,
  type GameSummary,
} from '@entities/game';
import { createRequestChannel, type RequestChannel } from '@shared/requests';

export function buildGameCardsQueryKey(
  searchQuery: string,
  selectedLibraries: readonly string[],
  selectedLaunchers: readonly string[],
  showHidden: boolean,
  favoritesOnly: boolean,
): string {
  return JSON.stringify({
    searchQuery,
    selectedLibraries,
    selectedLaunchers,
    showHidden,
    favoritesOnly,
  });
}

export type GamesQuerySnapshot = {
  requestKey: string;
  searchQuery: string;
  selectedLibraries: string[];
  selectedLaunchers: string[];
  showHidden: boolean;
  favoritesOnly: boolean;
};

export type GamesQueryResultSinks = {
  setItems(items: GameSummary[]): void;
  setHiddenCount(count: number): void;
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
  selectedLaunchers: readonly string[],
  showHidden: boolean,
  favoritesOnly: boolean,
) {
  return `${version}:${buildGameCardsQueryKey(searchQuery, selectedLibraries, selectedLaunchers, showHidden, favoritesOnly)}`;
}

export function createGamesPageQueryScheduler(options: SchedulerOptions = {}) {
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
    selectedLaunchers: readonly string[],
    showHidden: boolean,
    favoritesOnly: boolean,
  ): GamesQuerySnapshot | null {
    if (!filtersReady || !preferenceLoaded) {
      return null;
    }

    const normalizedSearchQuery = searchQuery.trim();
    const normalizedSelectedLibraries = [...selectedLibraries];
    const normalizedSelectedLaunchers = [...selectedLaunchers];

    return {
      requestKey: createRequestKey(
        version,
        normalizedSearchQuery,
        normalizedSelectedLibraries,
        normalizedSelectedLaunchers,
        showHidden,
        favoritesOnly,
      ),
      searchQuery: normalizedSearchQuery,
      selectedLibraries: normalizedSelectedLibraries,
      selectedLaunchers: normalizedSelectedLaunchers,
      showHidden,
      favoritesOnly,
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
        selectedLaunchers: snapshot.selectedLaunchers,
        showHidden: snapshot.showHidden,
        favoritesOnly: snapshot.favoritesOnly,
        sort: DEFAULT_GAME_CARDS_CATALOG_SORT,
        page: DEFAULT_GAME_CARDS_CATALOG_PAGE,
      });

      if (!requests.isActive(requestId)) {
        return;
      }

      sinks.setItems(result.items);
      sinks.setHiddenCount(result.hiddenCount);
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
