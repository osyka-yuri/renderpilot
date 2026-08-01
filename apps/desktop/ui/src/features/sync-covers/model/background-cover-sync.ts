import { type CoverArtworkResult, type GameSummary } from '@entities/game';
import { t } from '@shared/i18n';
import {
  STEAMGRIDDB_SETTING_KEY,
  fetchCoverRemotePolicy,
  fetchSteamGridDbKeyConfigured,
} from '@entities/settings';
import {
  filterGamesMissingStoredCoverForBackgroundSync,
  formatCoverSyncBanner,
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

export function formatBackgroundCoverSyncError(): string {
  return t('coverSync.failed');
}

export async function executeBackgroundCoverSync(
  games: readonly GameSummary[],
  options: {
    readSetting: CatalogSettingReader;
    fetchGameCover: (gameId: string) => Promise<CoverArtworkResult>;
    onGameStart: (gameId: string) => void;
    onGameEnd: (gameId: string) => void;
    onError: (message: string) => void;
    /**
     * Fired after each cover finishes downloading successfully, so callers can refresh that
     * card right away instead of waiting for the whole batch. Failed downloads do not fire it.
     */
    onCoverReady?: (gameId: string, result: CoverArtworkResult) => void;
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
    onCoverReady: (gameId, result) => options.onCoverReady?.(gameId, result),
  });

  const message = formatCoverSyncBanner(failures);

  if (message !== null) {
    options.onError(message);
  }
}
