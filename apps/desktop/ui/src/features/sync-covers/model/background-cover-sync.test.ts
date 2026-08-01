import { describe, expect, it, vi } from 'vitest';
import { LAUNCHER_STEAM, LAUNCHER_GOG, type GameSummary, createGameSummary } from '@entities/game';
import { t } from '@shared/i18n';
import {
  formatBackgroundCoverSyncError,
  executeBackgroundCoverSync,
} from './background-cover-sync';

const COVER_RESULT = { file_name: 'cover.webp', updated_at_ms: 123 };

function gameWithCover(overrides: Partial<GameSummary> = {}): GameSummary {
  return createGameSummary({
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
  });
}

function gameWithoutCover(overrides: Partial<GameSummary> = {}): GameSummary {
  return createGameSummary({
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
  });
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
  describe('formatBackgroundCoverSyncError', () => {
    it('returns the localized background-sync failure message', () => {
      expect(formatBackgroundCoverSyncError()).toBe(t('coverSync.failed'));
    });
  });

  describe('executeBackgroundCoverSync', () => {
    it('does nothing when all games already have covers', async () => {
      const fetchGameCover = vi.fn(() => Promise.resolve(COVER_RESULT));
      const onError = vi.fn();

      await executeBackgroundCoverSync([gameWithCover()], {
        readSetting: vi.fn(() => Promise.resolve({ value: 'false' })),
        fetchGameCover,
        onGameStart: vi.fn(),
        onGameEnd: vi.fn(),
        onError,
      });

      expect(fetchGameCover).not.toHaveBeenCalled();
      expect(onError).not.toHaveBeenCalled();
    });

    it('fetches covers for missing games and reports combined errors', async () => {
      const fetchGameCover = vi.fn((gameId: string) => {
        if (gameId === 'steam-game') {
          return Promise.resolve(COVER_RESULT);
        }
        return Promise.reject(new Error('fetch failed'));
      });
      const onGameStart = vi.fn();
      const onGameEnd = vi.fn();
      const onCoverReady = vi.fn();
      const onError = vi.fn();

      await executeBackgroundCoverSync([steamGame(), gogGame()], {
        readSetting: vi.fn((key: string) => {
          if (key.includes('steam_cdn') || key.includes('gog_cdn')) {
            return Promise.resolve({ value: 'true' });
          }
          return Promise.resolve({ value: 'false' });
        }),
        fetchGameCover,
        onGameStart,
        onGameEnd,
        onCoverReady,
        onError,
      });

      expect(onGameStart).toHaveBeenCalledTimes(2);
      expect(onGameEnd).toHaveBeenCalledTimes(2);
      // Only the successful download fires onCoverReady; the failed one does not.
      expect(onCoverReady).toHaveBeenCalledTimes(1);
      expect(onCoverReady).toHaveBeenCalledWith('steam-game', COVER_RESULT);
      expect(onError).toHaveBeenCalledWith(expect.stringContaining('Could not download'));
    });

    it('fires onCoverReady progressively, once per successfully downloaded cover', async () => {
      const fetchGameCover = vi.fn(() => Promise.resolve(COVER_RESULT));
      const onCoverReady = vi.fn();
      const onError = vi.fn();

      await executeBackgroundCoverSync([steamGame(), gogGame({ external_id: '789' })], {
        readSetting: vi.fn((key: string) => {
          if (key.includes('steam_cdn') || key.includes('gog_cdn')) {
            return Promise.resolve({ value: 'true' });
          }
          return Promise.resolve({ value: 'false' });
        }),
        fetchGameCover,
        onGameStart: vi.fn(),
        onGameEnd: vi.fn(),
        onCoverReady,
        onError,
      });

      // Each card is patched as its cover arrives; no catalog-wide refresh is needed.
      expect(onCoverReady).toHaveBeenCalledTimes(2);
      expect(onCoverReady).toHaveBeenCalledWith('steam-game', COVER_RESULT);
      expect(onCoverReady).toHaveBeenCalledWith('gog-game', COVER_RESULT);
      expect(onError).not.toHaveBeenCalled();
    });
  });
});
