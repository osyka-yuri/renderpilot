import type { WorkspaceScreen } from '@app/navigation/workspace';
import {
  getGameDetails,
  normalizeSelectableGameId,
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
  refreshCatalogCapabilities,
  refreshRemoteManifests,
  scanAutoLibrariesWithErrorRecovery,
  selectManualScanFolder,
  scanManualFolder,
} from '@features/scan-libraries';
import { describeCommandErrorBrief } from '@shared/api';

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
  preloadPage: () => void;
  runExclusive: <T>(task: () => Promise<T>) => Promise<T | null>;
  loadGameDetails: (gameId: string, nextScreen: WorkspaceScreen) => Promise<void>;
  normalizeGameId?: (gameId: string) => string;
};

/**
 * Starts loading the target page before acquiring the exclusive lock and fetching
 * details, so code loading and IPC can proceed in parallel.
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

  deps.preloadPage();
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

type QueueBackgroundCoverSyncDeps = Pick<
  CatalogRefreshWithCoverSyncDeps,
  'coverSyncQueue' | 'syncMissingCoversAfterCardsLoad'
>;

/** Coalesces background cover hydration through the process-wide UI queue. */
export function queueBackgroundCoverSync(deps: QueueBackgroundCoverSyncDeps): void {
  deps.coverSyncQueue.queue(deps.syncMissingCoversAfterCardsLoad, (error) => {
    publishBackgroundCoverSyncFailureNotification(error);
  });
}

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
    queueBackgroundCoverSync(deps);
  }
}

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

/** Capability refresh is best-effort; the last durable snapshot remains valid on failure. */
async function refreshCatalogCapabilitiesBestEffort(
  refresh: () => Promise<unknown> = refreshCatalogCapabilities,
): Promise<void> {
  try {
    await refresh();
  } catch (error) {
    console.error(
      `Catalog capability refresh failed; keeping the previous snapshot. ${describeCommandErrorBrief(error)}`,
      error,
    );
  }
}

export type UserCatalogRefreshDeps = CatalogRefreshWithCoverSyncDeps & {
  /** Optional override for the forced remote-manifest refresh. */
  refreshRemoteManifests?: () => Promise<unknown>;
  /** Optional override for rebuilding durable capability facts. */
  refreshCatalogCapabilities?: () => Promise<unknown>;
};

/**
 * Shell Refresh: force remote CDN manifests (cooldown-gated), then auto-scan
 * libraries, then refresh cards + cover sync. Manifest failures never abort
 * the disk scan. Force runs inside the exclusive catalog lock.
 */
export async function runUserCatalogRefresh(deps: UserCatalogRefreshDeps): Promise<void> {
  await runCatalogRefreshWithCoverSync(async () => {
    await forceRemoteManifestsBestEffort(deps.refreshRemoteManifests);
    const shouldRefresh = await prepareAutoLibraryScan();
    if (shouldRefresh) {
      await refreshCatalogCapabilitiesBestEffort(deps.refreshCatalogCapabilities);
    }
    return shouldRefresh;
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
    await refreshCatalogCapabilitiesBestEffort();
    return true;
  }, deps);
}

export type SyncMissingCoversDeps = {
  games: readonly GameSummary[];
  readSetting: (key: string) => Promise<CatalogSettingPayload>;
  fetchGameCover: (gameId: string) => Promise<CoverArtworkResult>;
  coverSyncQueue: CoverSyncQueue;
  onCoverReady: (gameId: string, result: CoverArtworkResult) => void;
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
    onGameStart: (gameId) => {
      deps.coverSyncQueue.setAutoFetching(gameId, true);
    },
    onGameEnd: (gameId) => {
      deps.coverSyncQueue.setAutoFetching(gameId, false);
    },
    onCoverReady: (gameId, result) => {
      deps.onCoverReady(gameId, result);
    },
    onError: publishBackgroundCoverSyncIssueNotification,
  });
}
