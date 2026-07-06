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
import type { CoverArtworkResult } from '@entities/game';
import type { CatalogSettingPayload } from '@entities/settings';
import {
  executeBackgroundCoverSync,
  publishBackgroundCoverSyncFailureNotification,
  publishBackgroundCoverSyncIssueNotification,
  type CoverSyncQueue,
} from '@features/sync-covers';
import {
  publishAutomaticLibraryScanFailedNotification,
  publishPartialLibraryScanWarning,
  refreshRemoteManifests,
  scanAutoLibrariesWithErrorRecovery,
  selectManualScanFolder,
  scanManualFolder,
} from '@features/scan-libraries';
import { describeCommandErrorBrief } from '@shared/api';

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
  /** Callback when the currently selected game is no longer in the catalog. */
  clearSelectionIfSelectedGameMissing: () => void;
};

/**
 * Refreshes the main games catalog by querying the backend with default parameters
 * and updating the local application state.
 */
export async function refreshDesktopCatalog(deps: RefreshDesktopCatalogDeps): Promise<void> {
  const result = await (deps.queryGameCards ?? queryGameCards)({
    searchQuery: '',
    selectedLibraries: [],
    selectedAddons: [],
    selectedLaunchers: [],
    showHidden: false,
    favoritesOnly: false,
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

/**
 * Fetches details for a game and presents them, ignoring stale concurrent requests.
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

export type OpenDesktopGameDeps = {
  runExclusive: <T>(task: () => Promise<T>) => Promise<T | null>;
  loadGameDetails: (gameId: string, nextScreen: WorkspaceScreen) => Promise<void>;
  normalizeGameId?: (gameId: string) => string;
};

/**
 * Opens a game in the workspace after normalizing its id and acquiring the exclusive lock.
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

export type ReloadSelectedGameDeps = {
  selectedGameId: string | null;
  loadGameDetails: (gameId: string, nextScreen: WorkspaceScreen) => Promise<void>;
};

/** Reloads the selected game when one is selected; no-op otherwise. */
export async function reloadSelectedGame(
  nextScreen: WorkspaceScreen,
  deps: ReloadSelectedGameDeps,
): Promise<void> {
  if (deps.selectedGameId === null) {
    return;
  }

  await deps.loadGameDetails(deps.selectedGameId, nextScreen);
}

export type CatalogRefreshWithCoverSyncDeps = {
  runExclusive: <T>(task: () => Promise<T>) => Promise<T | null>;
  refreshGameCards: () => Promise<void>;
  coverSyncQueue: CoverSyncQueue;
  syncMissingCoversAfterCardsLoad: () => Promise<void>;
};

/**
 * Runs a prepare step under the exclusive lock, refreshes the catalog when it
 * succeeds, then queues background cover sync (outside the exclusive lock).
 *
 * `prepareRefresh` returns `false` to cancel without refreshing (e.g. user
 * dismissed the folder picker).
 */
export async function runCatalogRefreshWithCoverSync(
  prepareRefresh: () => Promise<boolean>,
  deps: CatalogRefreshWithCoverSyncDeps,
): Promise<void> {
  const refreshed = await deps.runExclusive(async () => {
    const shouldRefresh = await prepareRefresh();

    if (!shouldRefresh) {
      return false;
    }

    await deps.refreshGameCards();
    return true;
  });

  if (refreshed === true) {
    deps.coverSyncQueue.queue(deps.syncMissingCoversAfterCardsLoad, (error) => {
      publishBackgroundCoverSyncFailureNotification(error);
    });
  }
}

export type ScanAutoLibrariesAndRefreshDeps = CatalogRefreshWithCoverSyncDeps;

/**
 * Runs auto library scan with recovery notifications.
 * Always returns `true` so cards still refresh after a soft scan failure.
 */
async function prepareAutoLibraryScan(): Promise<boolean> {
  const scanResult = await scanAutoLibrariesWithErrorRecovery();

  if (scanResult.kind === 'error') {
    publishAutomaticLibraryScanFailedNotification(scanResult.message);
    return true;
  }

  if (scanResult.errors.length > 0) {
    publishPartialLibraryScanWarning(scanResult.errors.length);
  }

  return true;
}

/** Best-effort forced CDN manifest refresh; never throws. */
async function forceRemoteManifestsBestEffort(
  force: () => Promise<unknown> = refreshRemoteManifests,
): Promise<void> {
  try {
    await force();
  } catch (error) {
    // Silent for UX; keep a brief log for diagnostics. Disk scan must continue.
    console.error(
      `Remote manifest refresh failed; continuing with library scan. ${describeCommandErrorBrief(error)}`,
      error,
    );
  }
}

/** Scans auto libraries (with recovery), then refreshes cards + cover sync. */
export async function scanAutoLibrariesAndRefreshCards(
  deps: ScanAutoLibrariesAndRefreshDeps,
): Promise<void> {
  await runCatalogRefreshWithCoverSync(prepareAutoLibraryScan, deps);
}

export type UserCatalogRefreshDeps = CatalogRefreshWithCoverSyncDeps & {
  /** Optional override for the forced remote-manifest refresh. */
  refreshRemoteManifests?: () => Promise<unknown>;
};

/**
 * Shell Refresh: force remote CDN manifests (cooldown-gated), then auto-scan
 * libraries, then refresh cards + cover sync. Manifest failures never abort
 * the disk scan. Force runs inside the exclusive catalog lock.
 */
export async function runUserCatalogRefresh(deps: UserCatalogRefreshDeps): Promise<void> {
  await runCatalogRefreshWithCoverSync(async () => {
    await forceRemoteManifestsBestEffort(deps.refreshRemoteManifests);
    return prepareAutoLibraryScan();
  }, deps);
}

export type ManualScanAndRefreshDeps = CatalogRefreshWithCoverSyncDeps;

/** Manual folder scan flow: picker → scan → catalog refresh → cover sync. */
export async function scanManualFolderAndRefreshCards(
  deps: ManualScanAndRefreshDeps,
): Promise<void> {
  await runCatalogRefreshWithCoverSync(async () => {
    const selectedFolder = await selectManualScanFolder();

    if (selectedFolder === null) {
      return false;
    }

    await scanManualFolder(selectedFolder);
    return true;
  }, deps);
}

export type SyncMissingCoversDeps = {
  games: readonly GameSummary[];
  readSetting: (key: string) => Promise<CatalogSettingPayload>;
  fetchGameCover: (gameId: string) => Promise<CoverArtworkResult>;
  refreshGameCards: () => Promise<void>;
  coverSyncQueue: CoverSyncQueue;
  onCoverReady: () => void;
  /** Yield before snapshotting cards (e.g. `tick()` so the UI paints first). */
  beforeSync?: () => Promise<void>;
};

/**
 * Background cover hydration for the current catalog snapshot.
 * Isolated from the Svelte route so DesktopApp stays wiring-only.
 */
export async function syncMissingCoversAfterCardsLoad(deps: SyncMissingCoversDeps): Promise<void> {
  if (deps.beforeSync) {
    await deps.beforeSync();
  }

  const cardSnapshot = deps.games.slice();

  if (cardSnapshot.length === 0) {
    return;
  }

  await executeBackgroundCoverSync(cardSnapshot, {
    readSetting: deps.readSetting,
    fetchGameCover: deps.fetchGameCover,
    refreshGameCards: deps.refreshGameCards,
    onGameStart: (gameId) => {
      deps.coverSyncQueue.setAutoFetching(gameId, true);
    },
    onGameEnd: (gameId) => {
      deps.coverSyncQueue.setAutoFetching(gameId, false);
    },
    onCoverReady: deps.onCoverReady,
    onError: publishBackgroundCoverSyncIssueNotification,
  });
}
