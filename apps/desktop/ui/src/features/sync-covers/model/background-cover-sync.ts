import { type GameSummary } from '@entities/game';
import { describeCommandError } from '@shared/api';
import { t } from '@shared/i18n';
import {
  STEAMGRIDDB_SETTING_KEY,
  fetchCoverRemotePolicy,
  fetchSteamGridDbKeyConfigured,
} from '@entities/settings';
import {
  filterGamesMissingStoredCoverForBackgroundSync,
  formatCoverSyncBanner,
  combineCoverSyncMessages,
  runCoverFetchBatch,
  COVER_FETCH_CONCURRENCY,
} from './cover-sync';

type CatalogSettingReader = (key: string) => Promise<{ value: string | null }>;

export async function findGamesMissingStoredCovers(
  games: readonly GameSummary[],
  readSetting: CatalogSettingReader,
): Promise<GameSummary[]> {
  const [policy, hasSteamGridDbApiKey] = await Promise.all([
    fetchCoverRemotePolicy(readSetting),
    fetchSteamGridDbKeyConfigured(readSetting, STEAMGRIDDB_SETTING_KEY),
  ]);

  return filterGamesMissingStoredCoverForBackgroundSync(games, policy, hasSteamGridDbApiKey);
}

export async function refreshCardsAfterCoverSync(
  refreshGameCards: () => Promise<void>,
): Promise<string | null> {
  try {
    await refreshGameCards();
    return null;
  } catch (error) {
    return t('coverSync.refreshFailed', { error: describeCommandError(error) });
  }
}

export function formatBackgroundCoverSyncError(error: unknown): string {
  return t('coverSync.failed', { error: describeCommandError(error) });
}

export async function executeBackgroundCoverSync(
  games: readonly GameSummary[],
  options: {
    readSetting: CatalogSettingReader;
    fetchGameCover: (gameId: string) => Promise<unknown>;
    refreshGameCards: () => Promise<void>;
    onGameStart: (gameId: string) => void;
    onGameEnd: (gameId: string) => void;
    onError: (message: string) => void;
    /**
     * Fired after each cover finishes downloading successfully, so callers can refresh that
     * card right away instead of waiting for the whole batch. Failed downloads do not fire it.
     */
    onCoverReady?: (gameId: string) => void;
  },
): Promise<void> {
  const missingCoverCards = await findGamesMissingStoredCovers(games, options.readSetting);

  if (missingCoverCards.length === 0) {
    return;
  }

  const { failures } = await runCoverFetchBatch({
    games: missingCoverCards,
    concurrency: COVER_FETCH_CONCURRENCY,
    fetchCover: options.fetchGameCover,
    onGameStart: options.onGameStart,
    onGameEnd: options.onGameEnd,
    onCoverReady: options.onCoverReady,
  });

  const refreshError = await refreshCardsAfterCoverSync(options.refreshGameCards);
  const combinedMessage = combineCoverSyncMessages(formatCoverSyncBanner(failures), refreshError);

  if (combinedMessage !== null) {
    options.onError(combinedMessage);
  }
}
