import {
  queryGameCards,
  DEFAULT_GAME_CARDS_CATALOG_PAGE,
  DEFAULT_GAME_CARDS_CATALOG_SORT,
  type AddonCapability,
  type GameSummary,
} from '@entities/game';
import { reportClientError } from '@shared/errors';
import { createRequestChannel, type RequestChannel } from '@shared/requests';

export function buildGameCardsQueryKey(
  searchQuery: string,
  selectedLibraries: readonly string[],
  selectedAddons: readonly AddonCapability[],
  selectedLaunchers: readonly string[],
  showHidden: boolean,
  favoritesOnly: boolean,
  launcherOrder: readonly string[] = [],
  pageOffset = 0,
): string {
  return JSON.stringify({
    searchQuery,
    selectedLibraries,
    selectedAddons,
    selectedLaunchers,
    showHidden,
    favoritesOnly,
    launcherOrder,
    sort: DEFAULT_GAME_CARDS_CATALOG_SORT,
    page: { ...DEFAULT_GAME_CARDS_CATALOG_PAGE, offset: pageOffset },
  });
}

export type GamesQuerySnapshot = {
  requestKey: string;
  searchQuery: string;
  selectedLibraries: string[];
  selectedAddons: AddonCapability[];
  selectedLaunchers: string[];
  launcherOrder: string[];
  showHidden: boolean;
  favoritesOnly: boolean;
  pageOffset: number;
  requiredCatalogRevision?: number;
};

export type GamesQueryResultSinks = {
  setItems(items: GameSummary[]): void;
  setCatalogSize?(size: number): void;
  setHiddenCount(count: number): void;
  setCatalogRevision?(revision: number): void;
  setNextOffset?(offset: number | null): void;
  onCatalogRevisionMismatch?(actualRevision: number): void;
};

type SchedulerOptions = {
  queryGameCardsFn?: typeof queryGameCards;
  requests?: RequestChannel;
};

const EMPTY_REQUEST_KEY = '';

function normalizeSemanticSelection<T extends string>(values: readonly T[]): T[] {
  return Array.from(new Set(values)).sort((left, right) =>
    left < right ? -1 : left > right ? 1 : 0,
  );
}

function createRequestKey(
  version: number,
  searchQuery: string,
  selectedLibraries: readonly string[],
  selectedAddons: readonly AddonCapability[],
  selectedLaunchers: readonly string[],
  showHidden: boolean,
  favoritesOnly: boolean,
  launcherOrder: readonly string[],
  pageOffset: number,
) {
  return `${version}:${buildGameCardsQueryKey(
    searchQuery,
    selectedLibraries,
    selectedAddons,
    selectedLaunchers,
    showHidden,
    favoritesOnly,
    launcherOrder,
    pageOffset,
  )}`;
}

export function createGamesPageQueryScheduler(options: SchedulerOptions = {}) {
  const fetchCards = options.queryGameCardsFn ?? queryGameCards;
  const requests = options.requests ?? createRequestChannel();

  let lastHandledRequestKey = EMPTY_REQUEST_KEY;
  let activeRequestKey = EMPTY_REQUEST_KEY;
  let desiredRequestKey = EMPTY_REQUEST_KEY;
  let trailingRequest: { snapshot: GamesQuerySnapshot; sinks: GamesQueryResultSinks } | null = null;
  let activeDrain: Promise<void> | null = null;
  let latestAcceptedCatalogRevision = 0;

  function createGamesQuerySnapshot(
    version: number,
    filtersReady: boolean,
    preferenceLoaded: boolean,
    searchQuery: string,
    selectedLibraries: readonly string[],
    selectedAddons: readonly AddonCapability[],
    selectedLaunchers: readonly string[],
    showHidden: boolean,
    favoritesOnly: boolean,
    launcherOrder: readonly string[] = [],
    pageOffset = 0,
  ): GamesQuerySnapshot | null {
    if (!filtersReady || !preferenceLoaded) {
      return null;
    }

    const normalizedSearchQuery = searchQuery.trim().toLowerCase();
    const normalizedSelectedLibraries = normalizeSemanticSelection(selectedLibraries);
    const normalizedSelectedAddons = normalizeSemanticSelection(selectedAddons);
    const normalizedSelectedLaunchers = normalizeSemanticSelection(selectedLaunchers);
    const normalizedLauncherOrder = [...launcherOrder];

    return {
      requestKey: createRequestKey(
        version,
        normalizedSearchQuery,
        normalizedSelectedLibraries,
        normalizedSelectedAddons,
        normalizedSelectedLaunchers,
        showHidden,
        favoritesOnly,
        normalizedLauncherOrder,
        pageOffset,
      ),
      searchQuery: normalizedSearchQuery,
      selectedLibraries: normalizedSelectedLibraries,
      selectedAddons: normalizedSelectedAddons,
      selectedLaunchers: normalizedSelectedLaunchers,
      launcherOrder: normalizedLauncherOrder,
      showHidden,
      favoritesOnly,
      pageOffset,
    };
  }

  function createPageQuerySnapshot(
    base: GamesQuerySnapshot,
    pageOffset: number,
    requiredCatalogRevision: number,
  ): GamesQuerySnapshot {
    return {
      ...base,
      requestKey: `${base.requestKey}:page:${pageOffset}:revision:${requiredCatalogRevision}`,
      pageOffset,
      requiredCatalogRevision,
    };
  }

  function createRevisionRestartSnapshot(
    base: GamesQuerySnapshot,
    observedCatalogRevision: number,
  ): GamesQuerySnapshot {
    const pageOffset = 0;
    return {
      ...base,
      requestKey: `${base.requestKey}:restart:${observedCatalogRevision}`,
      pageOffset,
      requiredCatalogRevision: undefined,
    };
  }

  function canRunGamesQuery(requestKey: string): boolean {
    return (
      requestKey !== lastHandledRequestKey &&
      requestKey !== activeRequestKey &&
      requestKey !== trailingRequest?.snapshot.requestKey
    );
  }

  async function executeGamesQuery(
    snapshot: GamesQuerySnapshot,
    sinks: GamesQueryResultSinks,
  ): Promise<void> {
    const requestId = requests.begin();
    activeRequestKey = snapshot.requestKey;

    try {
      const result = await fetchCards({
        searchQuery: snapshot.searchQuery,
        selectedLibraries: snapshot.selectedLibraries,
        selectedAddons: snapshot.selectedAddons,
        selectedLaunchers: snapshot.selectedLaunchers,
        launcherOrder: snapshot.launcherOrder,
        showHidden: snapshot.showHidden,
        favoritesOnly: snapshot.favoritesOnly,
        sort: DEFAULT_GAME_CARDS_CATALOG_SORT,
        page: { ...DEFAULT_GAME_CARDS_CATALOG_PAGE, offset: snapshot.pageOffset },
      });

      if (
        !requests.isActive(requestId) ||
        desiredRequestKey !== snapshot.requestKey ||
        result.catalogRevision < latestAcceptedCatalogRevision
      ) {
        return;
      }

      if (
        snapshot.requiredCatalogRevision !== undefined &&
        result.catalogRevision !== snapshot.requiredCatalogRevision
      ) {
        sinks.onCatalogRevisionMismatch?.(result.catalogRevision);
        return;
      }

      latestAcceptedCatalogRevision = result.catalogRevision;
      sinks.setItems(result.items);
      sinks.setCatalogSize?.(result.catalogSize);
      sinks.setHiddenCount(result.hiddenCount);
      sinks.setCatalogRevision?.(result.catalogRevision);
      sinks.setNextOffset?.(result.nextOffset);
    } catch (error: unknown) {
      if (requests.isActive(requestId) && desiredRequestKey === snapshot.requestKey) {
        reportClientError('query_game_cards', error);
      }
    } finally {
      const isCurrentRequest = requests.isActive(requestId);

      if (isCurrentRequest && desiredRequestKey === snapshot.requestKey) {
        lastHandledRequestKey = snapshot.requestKey;
      }

      if (activeRequestKey === snapshot.requestKey) {
        activeRequestKey = EMPTY_REQUEST_KEY;
      }
    }
  }

  async function drainGamesQueries(first: {
    snapshot: GamesQuerySnapshot;
    sinks: GamesQueryResultSinks;
  }): Promise<void> {
    let request: typeof first | null = first;
    while (request !== null) {
      await executeGamesQuery(request.snapshot, request.sinks);
      request = trailingRequest;
      trailingRequest = null;
    }
    activeDrain = null;
  }

  function runGamesQuery(
    snapshot: GamesQuerySnapshot,
    sinks: GamesQueryResultSinks,
  ): Promise<void> {
    if (!canRunGamesQuery(snapshot.requestKey)) {
      return activeDrain ?? Promise.resolve();
    }
    desiredRequestKey = snapshot.requestKey;
    if (activeDrain !== null) {
      trailingRequest = { snapshot, sinks };
      return activeDrain;
    }
    activeDrain = drainGamesQueries({ snapshot, sinks });
    return activeDrain;
  }

  return {
    createGamesQuerySnapshot,
    createPageQuerySnapshot,
    createRevisionRestartSnapshot,
    canRunGamesQuery,
    runGamesQuery,
  };
}
