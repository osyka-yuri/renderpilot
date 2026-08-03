import { describe, expect, it, vi } from 'vitest';
import {
  filterGamesMissingStoredCover,
  gameMayReceiveRemoteCoverViaPolicy,
  filterGamesMissingStoredCoverForBackgroundSync,
  runCoverFetchBatch,
  formatCoverSyncBanner,
  COVER_FETCH_CONCURRENCY,
} from './cover-sync';
import {
  catalogSettingHasSteamGridDbKey,
  parseCatalogBoolDefaultTrue,
  parseCatalogBoolWithDefault,
  fetchCoverRemotePolicy,
  fetchSteamGridDbKeyConfigured,
  COVERS_GOG_CDN_SETTING_KEY,
  COVERS_STEAM_CDN_SETTING_KEY,
  COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
  STEAMGRIDDB_SETTING_KEY,
} from '@entities/settings';
import { LAUNCHER_STEAM, LAUNCHER_GOG, type GameSummary, createGameSummary } from '@entities/game';
import { t } from '@shared/i18n';

describe('cover-sync', () => {
  describe('catalogSettingHasSteamGridDbKey', () => {
    it('returns false for null', () => {
      expect(catalogSettingHasSteamGridDbKey(null)).toBe(false);
    });

    it('returns false for empty string', () => {
      expect(catalogSettingHasSteamGridDbKey('')).toBe(false);
    });

    it('returns false for whitespace-only string', () => {
      expect(catalogSettingHasSteamGridDbKey('   ')).toBe(false);
    });

    it('returns true for non-blank string', () => {
      expect(catalogSettingHasSteamGridDbKey('my-api-key')).toBe(true);
    });
  });

  describe('parseCatalogBoolDefaultTrue', () => {
    it('returns true for null', () => {
      expect(parseCatalogBoolDefaultTrue(null)).toBe(true);
    });

    it('returns true for empty string', () => {
      expect(parseCatalogBoolDefaultTrue('')).toBe(true);
    });

    it('returns false for "false"', () => {
      expect(parseCatalogBoolDefaultTrue('false')).toBe(false);
    });

    it('returns false for "0"', () => {
      expect(parseCatalogBoolDefaultTrue('0')).toBe(false);
    });

    it('returns false for "no"', () => {
      expect(parseCatalogBoolDefaultTrue('no')).toBe(false);
    });

    it('returns false for "NO" (case-insensitive)', () => {
      expect(parseCatalogBoolDefaultTrue('NO')).toBe(false);
    });

    it('returns true for "true"', () => {
      expect(parseCatalogBoolDefaultTrue('true')).toBe(true);
    });

    it('returns true for "1"', () => {
      expect(parseCatalogBoolDefaultTrue('1')).toBe(true);
    });

    it('returns true for "yes"', () => {
      expect(parseCatalogBoolDefaultTrue('yes')).toBe(true);
    });
  });

  describe('parseCatalogBoolWithDefault', () => {
    it('returns the provided default for null', () => {
      expect(parseCatalogBoolWithDefault(null, false)).toBe(false);
      expect(parseCatalogBoolWithDefault(null, true)).toBe(true);
    });

    it('returns the provided default for an empty string', () => {
      expect(parseCatalogBoolWithDefault('', false)).toBe(false);
      expect(parseCatalogBoolWithDefault('', true)).toBe(true);
    });

    it('returns the provided default for a whitespace-only string', () => {
      expect(parseCatalogBoolWithDefault('   ', false)).toBe(false);
      expect(parseCatalogBoolWithDefault('   ', true)).toBe(true);
    });

    it('still honors disabling values regardless of the default', () => {
      expect(parseCatalogBoolWithDefault('false', true)).toBe(false);
      expect(parseCatalogBoolWithDefault('no', true)).toBe(false);
      expect(parseCatalogBoolWithDefault('0', true)).toBe(false);
      expect(parseCatalogBoolWithDefault('NO', true)).toBe(false);
    });

    it('enables for any value other than the disabling ones', () => {
      expect(parseCatalogBoolWithDefault('true', false)).toBe(true);
      expect(parseCatalogBoolWithDefault('yes', false)).toBe(true);
      expect(parseCatalogBoolWithDefault('1', false)).toBe(true);
      expect(parseCatalogBoolWithDefault('anything', false)).toBe(true);
    });
  });

  describe('fetchCoverRemotePolicy', () => {
    it('fetches all three settings and returns parsed booleans', async () => {
      const reader = vi.fn().mockImplementation((key: string) =>
        Promise.resolve({
          value: key === COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY ? 'false' : 'no',
        }),
      );

      const policy = await fetchCoverRemotePolicy(reader);

      expect(policy).toEqual({ steamCdn: false, gogCdn: false, steamgriddb: false });
      expect(reader).toHaveBeenCalledWith(COVERS_STEAM_CDN_SETTING_KEY);
      expect(reader).toHaveBeenCalledWith(COVERS_GOG_CDN_SETTING_KEY);
      expect(reader).toHaveBeenCalledWith(COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY);
    });

    it('defaults launcher CDNs to true but SteamGridDB to false when no key is set', async () => {
      const reader = vi.fn().mockResolvedValue({ value: null });

      const policy = await fetchCoverRemotePolicy(reader);

      // SteamGridDB cannot run without a key, so an unset toggle defaults to off.
      expect(policy).toEqual({ steamCdn: true, gogCdn: true, steamgriddb: false });
    });

    it('defaults SteamGridDB to true when a key is configured but the toggle is unset', async () => {
      const reader = vi.fn().mockImplementation((key: string) =>
        Promise.resolve({
          value: key === STEAMGRIDDB_SETTING_KEY ? 'my-key' : null,
        }),
      );

      const policy = await fetchCoverRemotePolicy(reader);

      expect(policy).toEqual({ steamCdn: true, gogCdn: true, steamgriddb: true });
    });

    it('honors an explicit SteamGridDB toggle regardless of key presence', async () => {
      const reader = vi.fn().mockImplementation((key: string) =>
        Promise.resolve({
          value: key === COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY ? 'false' : 'my-key',
        }),
      );

      const policy = await fetchCoverRemotePolicy(reader);

      expect(policy.steamgriddb).toBe(false);
    });
  });

  describe('fetchSteamGridDbKeyConfigured', () => {
    it('returns true when a non-blank key is stored', async () => {
      const reader = vi.fn().mockResolvedValue({ value: 'my-key' });

      const result = await fetchSteamGridDbKeyConfigured(reader, 'sgdb_key');

      expect(result).toBe(true);
      expect(reader).toHaveBeenCalledWith('sgdb_key');
    });

    it('returns false when key is blank', async () => {
      const reader = vi.fn().mockResolvedValue({ value: '' });

      const result = await fetchSteamGridDbKeyConfigured(reader, 'sgdb_key');

      expect(result).toBe(false);
    });
  });

  describe('filterGamesMissingStoredCover', () => {
    it('keeps only games without stored cover', () => {
      const games = [gameWithCover(), gameWithoutCover()];

      const result = filterGamesMissingStoredCover(games);

      expect(result).toHaveLength(1);
      expect(result[0].game_id).toBe('game-a');
    });

    it('returns empty array when all games have covers', () => {
      const games = [gameWithCover()];

      expect(filterGamesMissingStoredCover(games)).toEqual([]);
    });
  });

  describe('gameMayReceiveRemoteCoverViaPolicy', () => {
    const allEnabledPolicy = { steamCdn: true, gogCdn: true, steamgriddb: true };
    const allDisabledPolicy = { steamCdn: false, gogCdn: false, steamgriddb: false };

    it('returns true when SteamGridDB is enabled and key is present', () => {
      expect(gameMayReceiveRemoteCoverViaPolicy(gameWithoutCover(), allEnabledPolicy, true)).toBe(
        true,
      );
    });

    it('returns false when SteamGridDB is enabled but no key', () => {
      expect(gameMayReceiveRemoteCoverViaPolicy(gameWithoutCover(), allEnabledPolicy, false)).toBe(
        false,
      );
    });

    it('returns true for Steam game with external_id when steamCdn enabled', () => {
      expect(
        gameMayReceiveRemoteCoverViaPolicy(
          steamGame({ external_id: '123' }),
          { ...allDisabledPolicy, steamCdn: true },
          false,
        ),
      ).toBe(true);
    });

    it('returns false for Steam game when steamCdn disabled', () => {
      expect(
        gameMayReceiveRemoteCoverViaPolicy(
          steamGame({ external_id: '123' }),
          allDisabledPolicy,
          false,
        ),
      ).toBe(false);
    });

    it('returns true for GOG game with numeric id when gogCdn enabled', () => {
      expect(
        gameMayReceiveRemoteCoverViaPolicy(
          gogGame({ external_id: '123' }),
          { ...allDisabledPolicy, gogCdn: true },
          false,
        ),
      ).toBe(true);
    });

    it('returns false for GOG game when gogCdn disabled', () => {
      expect(
        gameMayReceiveRemoteCoverViaPolicy(
          gogGame({ external_id: '123' }),
          allDisabledPolicy,
          false,
        ),
      ).toBe(false);
    });
  });

  describe('filterGamesMissingStoredCoverForBackgroundSync', () => {
    it('returns only games missing covers that are eligible by policy', () => {
      const games = [
        gameWithCover(), // has cover, excluded
        steamGame({ external_id: '123' }), // missing cover, Steam CDN eligible
        gameWithoutCover(), // missing cover, not eligible (unknown launcher)
      ];

      const result = filterGamesMissingStoredCoverForBackgroundSync(
        games,
        { steamCdn: true, gogCdn: false, steamgriddb: false },
        false,
      );

      expect(result).toHaveLength(1);
      expect(result[0].game_id).toBe('steam-game');
    });
  });

  describe('runCoverFetchBatch', () => {
    it('returns empty failures when no games', async () => {
      const result = await runCoverFetchBatch({
        games: [],
        concurrency: COVER_FETCH_CONCURRENCY,
        fetchCover: vi.fn(),
      });

      expect(result.failures).toEqual([]);
    });

    it('calls fetchCover for every game', async () => {
      const games = [gameWithoutCover(), gameWithoutCover({ game_id: 'game-2' })];
      const fetchCover = vi.fn().mockResolvedValue(undefined);

      await runCoverFetchBatch({
        games,
        concurrency: 1,
        fetchCover,
      });

      expect(fetchCover).toHaveBeenCalledTimes(2);
    });

    it('reports failures when fetchCover rejects', async () => {
      const games = [
        gameWithoutCover({ game_id: 'game-1' }),
        gameWithoutCover({ game_id: 'game-2' }),
      ];
      const fetchCover = vi.fn().mockImplementation((gameId: string) => {
        if (gameId === 'game-1') {
          return Promise.reject(new Error('Network error'));
        }
        return Promise.resolve();
      });

      const result = await runCoverFetchBatch({
        games,
        concurrency: 1,
        fetchCover,
      });

      expect(result.failures).toHaveLength(1);
      expect(result.failures[0].gameId).toBe('game-1');
    });

    it('invokes lifecycle hooks including onCoverReady', async () => {
      const games = [gameWithoutCover()];
      const onGameStart = vi.fn();
      const onGameEnd = vi.fn();
      const onCoverReady = vi.fn();

      await runCoverFetchBatch({
        games,
        concurrency: 1,
        fetchCover: vi.fn().mockResolvedValue(undefined),
        onGameStart,
        onGameEnd,
        onCoverReady,
      });

      expect(onGameStart).toHaveBeenCalledWith('game-a');
      expect(onCoverReady).toHaveBeenCalledWith('game-a', undefined);
      expect(onGameEnd).toHaveBeenCalledWith('game-a');
    });

    it('does not invoke onCoverReady if fetchCover fails', async () => {
      const games = [gameWithoutCover()];
      const onCoverReady = vi.fn();

      await runCoverFetchBatch({
        games,
        concurrency: 1,
        fetchCover: vi.fn().mockRejectedValue(new Error('Network error')),
        onCoverReady,
      });

      expect(onCoverReady).not.toHaveBeenCalled();
    });

    it('does not downgrade a successful download when onCoverReady throws', async () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
      const games = [gameWithoutCover()];
      const onCoverReady = vi.fn(() => {
        throw new Error('refresh blew up');
      });

      const result = await runCoverFetchBatch({
        games,
        concurrency: 1,
        fetchCover: vi.fn().mockResolvedValue(undefined),
        onCoverReady,
      });

      // The cover did download, so it must not appear as a failure, and the batch must resolve.
      expect(onCoverReady).toHaveBeenCalledWith('game-a', undefined);
      expect(result.failures).toEqual([]);

      consoleError.mockRestore();
    });

    it('keeps downloading the rest of the batch when a lifecycle hook throws', async () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
      const games = [
        gameWithoutCover({ game_id: 'game-1' }),
        gameWithoutCover({ game_id: 'game-2' }),
      ];
      const onGameStart = vi.fn((gameId: string) => {
        if (gameId === 'game-1') {
          throw new Error('spinner blew up');
        }
      });
      const fetchCover = vi.fn().mockResolvedValue(undefined);

      const result = await runCoverFetchBatch({
        games,
        concurrency: 1,
        fetchCover,
        onGameStart,
      });

      expect(fetchCover).toHaveBeenCalledTimes(2);
      expect(result.failures).toEqual([]);

      consoleError.mockRestore();
    });

    it('limits concurrency', async () => {
      const games = Array.from({ length: 5 }, (_, i) => gameWithoutCover({ game_id: `game-${i}` }));
      let activeCount = 0;
      let maxActive = 0;

      const fetchCover = vi.fn().mockImplementation(async () => {
        activeCount += 1;
        maxActive = Math.max(maxActive, activeCount);
        await new Promise((resolve) => setTimeout(resolve, 10));
        activeCount -= 1;
      });

      await runCoverFetchBatch({
        games,
        concurrency: 2,
        fetchCover,
      });

      expect(maxActive).toBeLessThanOrEqual(2);
    });
  });

  describe('formatCoverSyncBanner', () => {
    it('returns null when failures is empty', () => {
      expect(formatCoverSyncBanner([])).toBeNull();
    });

    it('formats single failure', () => {
      const result = formatCoverSyncBanner([
        { gameId: 'g1', title: 'Game One', message: 'Not found' },
      ]);

      expect(result).toBe(
        `${t('coverSync.failure.single', {
          title: 'Game One',
          message: 'Not found',
        })}\n${t('coverSync.failure.hint')}`,
      );
    });

    it('formats multiple failures', () => {
      const result = formatCoverSyncBanner([
        { gameId: 'g1', title: 'Game One', message: 'Not found' },
        { gameId: 'g2', title: 'Game Two', message: 'Timeout' },
      ]);

      expect(result).toBe(
        `${t('coverSync.failure.multiple', {
          count: 2,
          summary: 'Game One: Not found',
        })}\n${t('coverSync.failure.hint')}`,
      );
    });
  });
});

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
    risk_level: 'safe',
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
    risk_level: 'safe',
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
    ...overrides,
  });
}

function gogGame(overrides: Partial<GameSummary> = {}): GameSummary {
  return gameWithoutCover({
    game_id: 'gog-game',
    title: 'GOG Game',
    launcher: LAUNCHER_GOG,
    ...overrides,
  });
}
