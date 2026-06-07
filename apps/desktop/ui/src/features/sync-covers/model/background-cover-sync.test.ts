import { describe, expect, it, vi } from 'vitest';
import { LAUNCHER_STEAM, LAUNCHER_GOG, type GameSummary } from '@entities/game';
import { t } from '@shared/i18n';
import {
  refreshCardsAfterCoverSync,
  formatBackgroundCoverSyncError,
  executeBackgroundCoverSync,
} from './background-cover-sync';

function gameWithCover(overrides: Partial<GameSummary> = {}): GameSummary {
  return {
    game_id: 'game-a',
    title: 'Game A',
    launcher: 'Unknown',
    platform: 'Windows',
    runtime: 'Native',
    install_path: 'C:/games/a',
    library_tags: [],
    component_count: 0,
    updates_available: false,
    update_count: 0,
    risk_level: 'unknown',
    rollback_available: false,
    operation_count: 0,
    cover_updated_at_ms: 1234567890,
    is_favorite: false,
    is_hidden: false,
    ...overrides,
  };
}

function gameWithoutCover(overrides: Partial<GameSummary> = {}): GameSummary {
  return {
    game_id: 'game-a',
    title: 'Game A',
    launcher: 'Unknown',
    platform: 'Windows',
    runtime: 'Native',
    install_path: 'C:/games/a',
    library_tags: [],
    component_count: 0,
    updates_available: false,
    update_count: 0,
    risk_level: 'unknown',
    rollback_available: false,
    operation_count: 0,
    is_favorite: false,
    is_hidden: false,
    ...overrides,
  };
}

function steamGame(overrides: Partial<GameSummary> = {}): GameSummary {
  return gameWithoutCover({
    game_id: 'steam-game',
    title: 'Steam Game',
    launcher: LAUNCHER_STEAM,
    external_id: '123',
    ...overrides,
  });
}

function gogGame(overrides: Partial<GameSummary> = {}): GameSummary {
  return gameWithoutCover({
    game_id: 'gog-game',
    title: 'GOG Game',
    launcher: LAUNCHER_GOG,
    external_id: '456',
    ...overrides,
  });
}

describe('background-cover-sync', () => {
  describe('refreshCardsAfterCoverSync', () => {
    it('returns null on success', async () => {
      const refreshGameCards = vi.fn(() => Promise.resolve());

      const result = await refreshCardsAfterCoverSync(refreshGameCards);

      expect(result).toBeNull();
    });

    it('returns error message on failure', async () => {
      const error = new Error('refresh failed');
      const refreshGameCards = vi.fn(() => Promise.reject(error));

      const result = await refreshCardsAfterCoverSync(refreshGameCards);

      expect(result).toBe(t('coverSync.refreshFailed'));
    });
  });

  describe('formatBackgroundCoverSyncError', () => {
    it('formats error with describeCommandError', () => {
      const error = new Error('network failure');

      const result = formatBackgroundCoverSyncError(error);

      expect(result).toBe(t('coverSync.failed'));
    });
  });

  describe('executeBackgroundCoverSync', () => {
    it('does nothing when all games already have covers', async () => {
      const fetchGameCover = vi.fn(() => Promise.resolve());
      const refreshGameCards = vi.fn(() => Promise.resolve());
      const onError = vi.fn();

      await executeBackgroundCoverSync([gameWithCover()], {
        readSetting: vi.fn(() => Promise.resolve({ value: 'false' })),
        fetchGameCover,
        refreshGameCards,
        onGameStart: vi.fn(),
        onGameEnd: vi.fn(),
        onError,
      });

      expect(fetchGameCover).not.toHaveBeenCalled();
      expect(refreshGameCards).not.toHaveBeenCalled();
      expect(onError).not.toHaveBeenCalled();
    });

    it('fetches covers for missing games and reports combined errors', async () => {
      const fetchGameCover = vi.fn((gameId: string) => {
        if (gameId === 'steam-game') return Promise.resolve();
        return Promise.reject(new Error('fetch failed'));
      });
      const refreshGameCards = vi.fn(() => Promise.resolve());
      const onGameStart = vi.fn();
      const onGameEnd = vi.fn();
      const onCoverReady = vi.fn();
      const onError = vi.fn();

      await executeBackgroundCoverSync([steamGame(), gogGame()], {
        readSetting: vi.fn((key: string) => {
          if (key.includes('steam_cdn') || key.includes('gog_cdn'))
            return Promise.resolve({ value: 'true' });
          return Promise.resolve({ value: 'false' });
        }),
        fetchGameCover,
        refreshGameCards,
        onGameStart,
        onGameEnd,
        onCoverReady,
        onError,
      });

      expect(onGameStart).toHaveBeenCalledTimes(2);
      expect(onGameEnd).toHaveBeenCalledTimes(2);
      expect(refreshGameCards).toHaveBeenCalledTimes(1);
      // Only the successful download fires onCoverReady; the failed one does not.
      expect(onCoverReady).toHaveBeenCalledTimes(1);
      expect(onCoverReady).toHaveBeenCalledWith('steam-game');
      expect(onError).toHaveBeenCalledWith(expect.stringContaining('Could not download'));
    });

    it('fires onCoverReady progressively, once per successfully downloaded cover', async () => {
      const fetchGameCover = vi.fn(() => Promise.resolve());
      const refreshGameCards = vi.fn(() => Promise.resolve());
      const onCoverReady = vi.fn();
      const onError = vi.fn();

      await executeBackgroundCoverSync([steamGame(), gogGame({ external_id: '789' })], {
        readSetting: vi.fn((key: string) => {
          if (key.includes('steam_cdn') || key.includes('gog_cdn'))
            return Promise.resolve({ value: 'true' });
          return Promise.resolve({ value: 'false' });
        }),
        fetchGameCover,
        refreshGameCards,
        onGameStart: vi.fn(),
        onGameEnd: vi.fn(),
        onCoverReady,
        onError,
      });

      // Each card refreshes as its cover arrives (progressive), and the batch still
      // performs exactly one final reconciliation refresh.
      expect(onCoverReady).toHaveBeenCalledTimes(2);
      expect(onCoverReady).toHaveBeenCalledWith('steam-game');
      expect(onCoverReady).toHaveBeenCalledWith('gog-game');
      expect(refreshGameCards).toHaveBeenCalledTimes(1);
      expect(onError).not.toHaveBeenCalled();
    });
  });
});
