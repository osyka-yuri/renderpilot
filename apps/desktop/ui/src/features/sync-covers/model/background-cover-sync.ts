import { type GameSummary } from '@entities/game';
import { describeCommandError } from '@shared/api';
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
    return `${describeCommandError(error)} (covers may have downloaded; try Refresh Libraries.)`;
  }
}

export function formatBackgroundCoverSyncError(error: unknown): string {
  return `Background cover sync failed. ${describeCommandError(error)}`;
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
  });

  const refreshError = await refreshCardsAfterCoverSync(options.refreshGameCards);
  const combinedMessage = combineCoverSyncMessages(formatCoverSyncBanner(failures), refreshError);

  if (combinedMessage !== null) {
    options.onError(combinedMessage);
  }
}
