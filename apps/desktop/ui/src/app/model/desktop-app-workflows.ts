import type { WorkspaceScreen } from '@app/navigation/workspace';
import {
  DEFAULT_GAME_CARDS_CATALOG_PAGE,
  DEFAULT_GAME_CARDS_CATALOG_SORT,
  getGameDetails,
  normalizeSelectableGameId,
  queryGameCards,
  type GameDetails,
  type GameSummary,
} from '@entities/game';

export type RefreshDesktopCatalogDeps = {
  queryGameCards?: typeof queryGameCards;
  setGames: (games: GameSummary[]) => void;
  incrementCatalogVersion: () => void;
  clearSelectionIfSelectedGameMissing: () => void;
};

export async function refreshDesktopCatalog(deps: RefreshDesktopCatalogDeps): Promise<void> {
  const result = await (deps.queryGameCards ?? queryGameCards)({
    searchQuery: '',
    selectedLibraries: [],
    selectedLaunchers: [],
    sort: DEFAULT_GAME_CARDS_CATALOG_SORT,
    page: DEFAULT_GAME_CARDS_CATALOG_PAGE,
  });

  deps.setGames(result.items);
  deps.incrementCatalogVersion();
  deps.clearSelectionIfSelectedGameMissing();
}

export type LoadAndPresentGameDetailsDeps<RequestToken> = {
  getGameDetails?: typeof getGameDetails;
  beginDetailsRequest: () => RequestToken;
  isDetailsRequestActive: (token: RequestToken) => boolean;
  presentGameDetails: (details: GameDetails, nextScreen: WorkspaceScreen) => void;
};

export async function loadAndPresentGameDetails<RequestToken>(
  gameId: string,
  nextScreen: WorkspaceScreen,
  deps: LoadAndPresentGameDetailsDeps<RequestToken>,
): Promise<void> {
  const requestToken = deps.beginDetailsRequest();
  const details = await (deps.getGameDetails ?? getGameDetails)(gameId);

  if (!deps.isDetailsRequestActive(requestToken)) {
    return;
  }

  deps.presentGameDetails(details, nextScreen);
}

export type OpenDesktopGameDeps = {
  runExclusive: <T>(task: () => Promise<T>) => Promise<T | null>;
  loadGameDetails: (gameId: string, nextScreen: WorkspaceScreen) => Promise<void>;
  normalizeGameId?: (gameId: string) => string;
};

export async function openDesktopGame(
  gameId: string,
  nextScreen: WorkspaceScreen,
  deps: OpenDesktopGameDeps,
): Promise<void> {
  const normalizedGameId = (deps.normalizeGameId ?? normalizeSelectableGameId)(gameId);

  if (normalizedGameId.length === 0) {
    return;
  }

  await deps.runExclusive(() => deps.loadGameDetails(normalizedGameId, nextScreen));
}

export type ReloadSelectedGameDeps = {
  selectedGameId: string | null;
  loadGameDetails: (gameId: string, nextScreen: WorkspaceScreen) => Promise<void>;
};

export async function reloadSelectedGame(
  nextScreen: WorkspaceScreen,
  deps: ReloadSelectedGameDeps,
): Promise<void> {
  if (deps.selectedGameId === null) {
    return;
  }

  await deps.loadGameDetails(deps.selectedGameId, nextScreen);
}
