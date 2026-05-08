/**
 * CONTRACT: Background-sync eligibility (`filterGamesMissingStoredCoverForBackgroundSync` /
 * `gameMayReceiveRemoteCoverViaPolicy`) must stay equivalent to remote resolution in
 * `crates/renderpilot-cli/src/catalog/covers/providers/resolve.rs` (`resolve_cover_bytes`).
 * When you change launcher branches or policy semantics on either side, update the other and
 * re-verify manually.
 */

import { describeCommandErrorBrief } from '@shared/api/errors';
import type { GameCard } from '@shared/api/types';
import {
  COVERS_GOG_CDN_SETTING_KEY,
  COVERS_STEAM_CDN_SETTING_KEY,
  COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
} from '@shared/catalog/catalog-setting-keys';
import { gameCardHasStoredCover } from '@shared/utils/game-card';

/**
 * Must match `Launcher` serde names from renderpilot-domain (`stable_enum!`, e.g. `Steam = "Steam"`).
 * Used so UI background-sync policy stays aligned with Rust `resolve_cover_bytes`.
 */
export const LAUNCHER_STEAM = 'Steam' as const;
export const LAUNCHER_GOG = 'Gog' as const;

/** Parallel downloads during background cover sync (launcher CDNs / SteamGridDB). */
export const COVER_FETCH_CONCURRENCY = 2;

const BOOL_DEFAULT_TRUE_DISABLED_VALUES = new Set(['false', '0', 'no']);
const GOG_NUMERIC_PRODUCT_ID_RE = /^\d+$/;

type CatalogSettingReader = (key: string) => Promise<{ value: string | null }>;

export type CoverRemotePolicy = {
  steamCdn: boolean;
  gogCdn: boolean;
  steamgriddb: boolean;
};

export type CoverFetchFailure = {
  gameId: string;
  title: string;
  message: string;
};

function trimNullable(value: string | null): string {
  return value?.trim() ?? '';
}

function getTrimmedExternalId(game: GameCard): string {
  return typeof game.external_id === 'string' ? game.external_id.trim() : '';
}

function getGameTitleOrId(game: GameCard): string {
  const title = game.title.trim();

  return title.length > 0 ? title : game.game_id;
}

function isPresent<T>(value: T | null | undefined): value is T {
  return value !== null && value !== undefined;
}

/** True when the catalog setting row holds a non-blank SteamGridDB bearer token. */
export function catalogSettingHasSteamGridDbKey(value: string | null): boolean {
  return trimNullable(value).length > 0;
}

/** Matches Rust `parse_setting_bool_default_true`: only false / 0 / no (any case) disables. */
export function parseCatalogBoolDefaultTrue(value: string | null): boolean {
  const normalized = trimNullable(value);

  if (normalized.length === 0) {
    return true;
  }

  return !BOOL_DEFAULT_TRUE_DISABLED_VALUES.has(normalized.toLowerCase());
}

async function readBoolSettingDefaultTrue(
  getCatalogSetting: CatalogSettingReader,
  key: string,
): Promise<boolean> {
  const { value } = await getCatalogSetting(key);

  return parseCatalogBoolDefaultTrue(value);
}

export async function fetchCoverRemotePolicy(
  getCatalogSetting: CatalogSettingReader,
): Promise<CoverRemotePolicy> {
  const [steamCdn, gogCdn, steamgriddb] = await Promise.all([
    readBoolSettingDefaultTrue(getCatalogSetting, COVERS_STEAM_CDN_SETTING_KEY),
    readBoolSettingDefaultTrue(getCatalogSetting, COVERS_GOG_CDN_SETTING_KEY),
    readBoolSettingDefaultTrue(getCatalogSetting, COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY),
  ]);

  return { steamCdn, gogCdn, steamgriddb };
}

export async function fetchSteamGridDbKeyConfigured(
  getCatalogSetting: CatalogSettingReader,
  settingKey: string,
): Promise<boolean> {
  const { value } = await getCatalogSetting(settingKey);

  return catalogSettingHasSteamGridDbKey(value);
}

export function filterGamesMissingStoredCover(games: readonly GameCard[]): GameCard[] {
  return games.filter((game) => !gameCardHasStoredCover(game));
}

/**
 * Without SteamGridDB, auto-fetch can still use first-party artwork when the catalog has a Steam app id
 * or GOG product id (numeric `external_id`).
 */
export function gameCoverFetchMayUseLauncherCdnOnly(game: GameCard): boolean {
  return gameHasSteamCdnCandidate(game) || gameHasGogCdnCandidate(game);
}

function gameHasSteamCdnCandidate(game: GameCard): boolean {
  return game.launcher === LAUNCHER_STEAM && getTrimmedExternalId(game).length > 0;
}

function gameHasGogCdnCandidate(game: GameCard): boolean {
  return (
    game.launcher === LAUNCHER_GOG && GOG_NUMERIC_PRODUCT_ID_RE.test(getTrimmedExternalId(game))
  );
}

/** Mirrors whether `resolve_cover_bytes` has any successful path for this card under `policy`. */
export function gameMayReceiveRemoteCoverViaPolicy(
  game: GameCard,
  policy: CoverRemotePolicy,
  hasSteamGridDbApiKey: boolean,
): boolean {
  if (policy.steamgriddb && hasSteamGridDbApiKey) {
    return true;
  }

  switch (game.launcher) {
    case LAUNCHER_STEAM:
      return policy.steamCdn && gameHasSteamCdnCandidate(game);

    case LAUNCHER_GOG:
      return policy.gogCdn && gameHasGogCdnCandidate(game);

    default:
      return false;
  }
}

export function filterGamesMissingStoredCoverForBackgroundSync(
  games: readonly GameCard[],
  policy: CoverRemotePolicy,
  hasSteamGridDbApiKey: boolean,
): GameCard[] {
  return filterGamesMissingStoredCover(games).filter((game) =>
    gameMayReceiveRemoteCoverViaPolicy(game, policy, hasSteamGridDbApiKey),
  );
}

/**
 * Runs `fetchCover` for each game with a bounded pool size. Invokes lifecycle hooks on the JS thread
 * around each attempt (for UI busy indicators).
 */
export async function runCoverFetchBatch(options: {
  games: readonly GameCard[];
  concurrency: number;
  fetchCover: (gameId: string) => Promise<void>;
  onGameStart?: (gameId: string) => void;
  onGameEnd?: (gameId: string) => void;
}): Promise<{ failures: CoverFetchFailure[] }> {
  const items = [...options.games];

  if (items.length === 0) {
    return { failures: [] };
  }

  const workerCount = getWorkerCount(options.concurrency, items.length);
  const failuresByInputIndex = new Array<CoverFetchFailure | undefined>(items.length);

  let nextIndex = 0;

  const claimNextIndex = (): number | null => {
    const current = nextIndex;
    nextIndex += 1;

    return current < items.length ? current : null;
  };

  const fetchOne = async (itemIndex: number): Promise<void> => {
    const game = items[itemIndex];
    const gameId = game.game_id;

    options.onGameStart?.(gameId);

    try {
      await options.fetchCover(gameId);
    } catch (error: unknown) {
      failuresByInputIndex[itemIndex] = createCoverFetchFailure(game, error);
    } finally {
      options.onGameEnd?.(gameId);
    }
  };

  const worker = async (): Promise<void> => {
    for (;;) {
      const itemIndex = claimNextIndex();

      if (itemIndex === null) {
        return;
      }

      await fetchOne(itemIndex);
    }
  };

  await Promise.all(Array.from({ length: workerCount }, () => worker()));

  return {
    failures: failuresByInputIndex.filter(isPresent),
  };
}

function getWorkerCount(concurrency: number, itemCount: number): number {
  const normalized = Math.floor(concurrency);

  if (!Number.isFinite(concurrency) || normalized < 1) {
    throw new RangeError('runCoverFetchBatch concurrency must be a positive finite number.');
  }

  return Math.min(normalized, itemCount);
}

function createCoverFetchFailure(game: GameCard, error: unknown): CoverFetchFailure {
  return {
    gameId: game.game_id,
    title: getGameTitleOrId(game),
    message: describeCommandErrorBrief(error),
  };
}

/** User-visible banner when some automatic cover downloads failed. */
export function formatCoverSyncBanner(failures: readonly CoverFetchFailure[]): string | null {
  const first = failures[0];

  const summary = `${first.title}: ${first.message}`;

  return `${failures.length} game(s): could not download a cover. First: ${summary}. Check Game artwork sources and SteamGridDB settings.`;
}

/** Merges background sync banner text with a post-sync refresh error, if any. */
export function combineCoverSyncMessages(
  syncBanner: string | null,
  refreshAfterSyncError: string | null,
): string | null {
  const messages = [syncBanner, refreshAfterSyncError].filter(isPresent);

  return messages.length > 0 ? messages.join(' ') : null;
}
