import type { WorkspaceScreen } from '@app/navigation/workspace';
import {
  getGameDetails,
  normalizeSelectableGameId,
  removeGameFromCatalog,
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
  publishAddGameWarnings,
  publishPartialLibraryScanWarning,
  refreshCatalogCapabilities,
  refreshRemoteManifests,
  scanAutoLibrariesWithErrorRecovery,
  addGame,
  type AddGameConfirmation,
  type AddGameInspection,
  type AddGameResult,
} from '@features/scan-libraries';
import { ClientError } from '@shared/errors';
import { t } from '@shared/i18n';
import { publishSuccessNotification } from '@shared/notifications';

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
  deps.coverSyncQueue.queue(deps.syncMissingCoversAfterCardsLoad, () => {
    publishBackgroundCoverSyncFailureNotification();
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

  if (scanResult.partialFailureCount > 0) {
    publishPartialLibraryScanWarning(scanResult.partialFailureCount);
  }

  return true;
}

/** Best-effort forced CDN manifest refresh; never throws. */
async function forceRemoteManifestsBestEffort(
  force: () => Promise<unknown> = refreshRemoteManifests,
): Promise<void> {
  try {
    await force();
  } catch {
    // `invokeDesktop` already emitted one safe diagnostic. Disk scan continues.
  }
}

/** Capability refresh is best-effort; the last durable snapshot remains valid on failure. */
async function refreshCatalogCapabilitiesBestEffort(
  refresh: () => Promise<unknown> = refreshCatalogCapabilities,
): Promise<void> {
  try {
    await refresh();
  } catch {
    // `invokeDesktop` already emitted one safe diagnostic; keep the previous snapshot.
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

export type SubmitAddGameDeps = CatalogRefreshWithCoverSyncDeps;

/**
 * Persists one already-confirmed inspection and refreshes the visible catalog.
 * Folder selection, review policy, and error presentation stay with the shell;
 * this function owns only the atomic catalog-side submission effects.
 */
export async function submitAddGameAndRefreshCards(
  inspection: AddGameInspection,
  confirmation: AddGameConfirmation,
  deps: SubmitAddGameDeps,
): Promise<AddGameResult | null> {
  const result = await deps.runExclusive(async () => {
    const added = await addGame({
      selectedRoot: inspection.selectedRoot,
      rootChoice: confirmation.rootChoice,
      allowRootCorrection: confirmation.allowRootCorrection,
      chosenExecutable: confirmation.chosenExecutable,
      inspectionFingerprint: inspection.inspectionFingerprint,
    });
    await refreshCatalogCapabilitiesBestEffort();
    await deps.refreshGameCards();
    return added;
  });
  if (result === null) {
    return null;
  }
  publishAddGameWarnings(result);
  queueBackgroundCoverSync(deps);
  return result;
}

export type RollbackRootCorrectionDeps = {
  rollbackComponent: (gameId: string, componentId: string) => Promise<unknown>;
  refreshGameCards: () => Promise<void>;
};

/**
 * Reverts the exact component set approved by the user before root correction.
 *
 * Rollbacks are deliberately sequential because each mutation owns the same
 * game lock and refreshes shared component state. The catalog is refreshed even
 * after a partial failure, while the original rollback error remains the
 * primary error shown to the user.
 */
export async function rollbackRootCorrectionComponents(
  gameId: string,
  componentIds: readonly string[],
  deps: RollbackRootCorrectionDeps,
): Promise<void> {
  let rollbackError: unknown = null;
  try {
    for (const componentId of componentIds) {
      await deps.rollbackComponent(gameId, componentId);
    }
  } catch (error) {
    rollbackError = error;
  }

  let refreshError: unknown = null;
  try {
    await deps.refreshGameCards();
  } catch (error) {
    refreshError = error;
  }

  if (rollbackError !== null) {
    throw asThrowable(rollbackError);
  }
  if (refreshError !== null) {
    throw asThrowable(refreshError);
  }
}

function asThrowable(error: unknown): Error {
  return error instanceof Error ? error : new ClientError('unexpected_client_error', error);
}

export type RemoveGameAndRefreshDeps = Pick<
  CatalogRefreshWithCoverSyncDeps,
  'runExclusive' | 'refreshGameCards'
> & {
  removeGame?: typeof removeGameFromCatalog;
};

/** Removes one user-managed card, then replaces the visible catalog snapshot. */
export async function removeGameAndRefreshCards(
  gameId: string,
  deps: RemoveGameAndRefreshDeps,
): Promise<boolean> {
  const result = await deps.runExclusive(async () => {
    await (deps.removeGame ?? removeGameFromCatalog)(gameId);
    await deps.refreshGameCards();
    return true;
  });
  if (result === true) {
    publishSuccessNotification(t('notify.gameRemovedFromCatalog'));
  }
  return result === true;
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
