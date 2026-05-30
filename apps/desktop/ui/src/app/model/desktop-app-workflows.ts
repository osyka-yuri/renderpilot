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

/**
 * Dependencies required for the refreshDesktopCatalog workflow.
 */
export type RefreshDesktopCatalogDeps = {
  /** Optional override for the queryGameCards API call. */
  queryGameCards?: typeof queryGameCards;
  /** Callback to update the games catalog state. */
  setGames: (games: GameSummary[]) => void;
  /** Callback to increment the catalog version for cache invalidation. */
  incrementCatalogVersion: () => void;
  /** Callback to handle the case where the currently selected game is no longer in the catalog. */
  clearSelectionIfSelectedGameMissing: () => void;
};

/**
 * Refreshes the main games catalog by querying the backend with default parameters
 * and updating the local application state.
 *
 * @param deps Injected dependencies for state management and API calls.
 */
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

/**
 * Dependencies required for the loadAndPresentGameDetails workflow.
 * @template RequestToken The type used to identify concurrent requests (e.g., a number or string).
 */
export type LoadAndPresentGameDetailsDeps<RequestToken> = {
  /** Optional override for the getGameDetails API call. */
  getGameDetails?: typeof getGameDetails;
  /** Marks the start of a details request and returns a unique request token. */
  beginDetailsRequest: () => RequestToken;
  /** Checks if the given token matches the currently active request. */
  isDetailsRequestActive: (token: RequestToken) => boolean;
  /** Callback to update the state with the loaded game details. */
  presentGameDetails: (details: GameDetails, nextScreen: WorkspaceScreen) => void;
};

/**
 * Fetches the details for a specific game and presents them in the workspace,
 * ignoring the result if a newer request has been initiated in the meantime.
 *
 * @param gameId The unique identifier of the game to load.
 * @param nextScreen The screen to display once details are loaded.
 * @param deps Injected dependencies for state and concurrency management.
 */
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

/**
 * Dependencies required for the openDesktopGame workflow.
 */
export type OpenDesktopGameDeps = {
  /** Ensures that the provided async task runs exclusively, preventing overlapping operations. */
  runExclusive: <T>(task: () => Promise<T>) => Promise<T | null>;
  /** Callback to load and present the game details. */
  loadGameDetails: (gameId: string, nextScreen: WorkspaceScreen) => Promise<void>;
  /** Optional function to normalize the game ID before loading. */
  normalizeGameId?: (gameId: string) => string;
};

/**
 * Initiates the opening of a game in the workspace. Normalizes the ID, handles concurrency,
 * and triggers the details loading process.
 *
 * @param gameId The raw game ID selected by the user.
 * @param nextScreen The screen to navigate to within the game workspace.
 * @param deps Injected dependencies for concurrency and loading.
 */
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

/**
 * Dependencies required for the reloadSelectedGame workflow.
 */
export type ReloadSelectedGameDeps = {
  /** The currently selected game ID, or null if nothing is selected. */
  selectedGameId: string | null;
  /** Callback to execute the game loading logic. */
  loadGameDetails: (gameId: string, nextScreen: WorkspaceScreen) => Promise<void>;
};

/**
 * Reloads the currently selected game if one exists, otherwise does nothing.
 *
 * @param nextScreen The screen to remain on or navigate to after reloading.
 * @param deps Injected dependencies providing current state and loading function.
 */
export async function reloadSelectedGame(
  nextScreen: WorkspaceScreen,
  deps: ReloadSelectedGameDeps,
): Promise<void> {
  if (deps.selectedGameId === null) {
    return;
  }

  await deps.loadGameDetails(deps.selectedGameId, nextScreen);
}
