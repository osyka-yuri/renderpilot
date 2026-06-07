/**
 * CONTRACT: Background-sync eligibility (`filterGamesMissingStoredCoverForBackgroundSync` /
 * `gameMayReceiveRemoteCoverViaPolicy`) must stay equivalent to remote resolution in
 * `crates/renderpilot-cli/src/catalog/covers/providers/resolve.rs` (`resolve_cover_bytes`).
 * When you change launcher branches or policy semantics on either side, update the other and
 * re-verify manually.
 */

import { describeCommandErrorBrief } from '@shared/api';
import { isDefined } from '@shared/validation';
import { type CoverRemotePolicy } from '@entities/settings';
import {
  gameCardHasStoredCover,
  LAUNCHER_STEAM,
  LAUNCHER_GOG,
  type GameSummary,
} from '@entities/game';

/** Parallel downloads during background cover sync (launcher CDNs / SteamGridDB). */
export const COVER_FETCH_CONCURRENCY = 2;

const GOG_NUMERIC_PRODUCT_ID_RE = /^\d+$/;

export type CoverFetchFailure = {
  gameId: string;
  title: string;
  message: string;
};

function getTrimmedExternalId(game: GameSummary): string {
  return typeof game.external_id === 'string' ? game.external_id.trim() : '';
}

function getGameTitleOrId(game: GameSummary): string {
  const title = game.title.trim();

  return title.length > 0 ? title : game.game_id;
}

export function filterGamesMissingStoredCover(games: readonly GameSummary[]): GameSummary[] {
  return games.filter((game) => !gameCardHasStoredCover(game));
}

/**
 * Without SteamGridDB, auto-fetch can still use first-party artwork when the catalog has a Steam app id
 * or GOG product id (numeric `external_id`).
 */
export function gameCoverFetchMayUseLauncherCdnOnly(game: GameSummary): boolean {
  return gameHasSteamCdnCandidate(game) || gameHasGogCdnCandidate(game);
}

function gameHasSteamCdnCandidate(game: GameSummary): boolean {
  return game.launcher === LAUNCHER_STEAM && getTrimmedExternalId(game).length > 0;
}

function gameHasGogCdnCandidate(game: GameSummary): boolean {
  return (
    game.launcher === LAUNCHER_GOG && GOG_NUMERIC_PRODUCT_ID_RE.test(getTrimmedExternalId(game))
  );
}

/** Mirrors whether `resolve_cover_bytes` has any successful path for this card under `policy`. */
export function gameMayReceiveRemoteCoverViaPolicy(
  game: GameSummary,
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
  games: readonly GameSummary[],
  policy: CoverRemotePolicy,
  hasSteamGridDbApiKey: boolean,
): GameSummary[] {
  return filterGamesMissingStoredCover(games).filter((game) =>
    gameMayReceiveRemoteCoverViaPolicy(game, policy, hasSteamGridDbApiKey),
  );
}

export type CoverFetchBatchHooks = {
  /** Fired before each attempt (success or failure) — typically a per-card busy spinner on. */
  onGameStart?: (gameId: string) => void;
  /** Fired after each attempt (success or failure) — typically a per-card busy spinner off. */
  onGameEnd?: (gameId: string) => void;
  /**
   * Fired once per *successful* download, after `onGameEnd`, so a single card can refresh the
   * moment its cover lands instead of waiting for the whole batch.
   */
  onCoverReady?: (gameId: string) => void;
};

export type CoverFetchBatchOptions = CoverFetchBatchHooks & {
  games: readonly GameSummary[];
  concurrency: number;
  fetchCover: (gameId: string) => Promise<unknown>;
};

/**
 * Runs `fetchCover` for every game through a bounded worker pool, collecting per-game failures
 * without aborting the batch.
 *
 * Lifecycle hooks ({@link CoverFetchBatchHooks}) report progress to the UI. They are invoked
 * defensively: a throwing hook is logged and isolated, so a buggy callback can neither abort the
 * remaining downloads nor be mistaken for a fetch failure.
 */
export async function runCoverFetchBatch(
  options: CoverFetchBatchOptions,
): Promise<{ failures: CoverFetchFailure[] }> {
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

    notifyLifecycleHook(options.onGameStart, gameId);

    let downloaded = false;

    try {
      await options.fetchCover(gameId);
      downloaded = true;
    } catch (error: unknown) {
      failuresByInputIndex[itemIndex] = createCoverFetchFailure(game, error);
    } finally {
      notifyLifecycleHook(options.onGameEnd, gameId);
    }

    // Notify success outside the fetch's try/catch: a throwing onCoverReady must never be
    // recorded as a download failure, and only confirmed downloads should trigger a refresh.
    if (downloaded) {
      notifyLifecycleHook(options.onCoverReady, gameId);
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
    failures: failuresByInputIndex.filter(isDefined),
  };
}

/**
 * Invokes an optional lifecycle hook, isolating the batch from a throwing UI callback.
 * Hooks are fire-and-forget side effects (busy flags, cache-version bumps); a failure in one
 * must not abort the remaining downloads or corrupt failure accounting.
 */
function notifyLifecycleHook(
  hook: ((gameId: string) => void) | undefined,
  gameId: string,
): void {
  if (hook === undefined) {
    return;
  }

  try {
    hook(gameId);
  } catch (error: unknown) {
    console.error('Cover sync lifecycle hook threw.', error);
  }
}

function getWorkerCount(concurrency: number, itemCount: number): number {
  const normalized = Math.floor(concurrency);

  if (!Number.isFinite(concurrency) || normalized < 1) {
    throw new RangeError('runCoverFetchBatch concurrency must be a positive finite number.');
  }

  return Math.min(normalized, itemCount);
}

function createCoverFetchFailure(game: GameSummary, error: unknown): CoverFetchFailure {
  return {
    gameId: game.game_id,
    title: getGameTitleOrId(game),
    message: describeCommandErrorBrief(error),
  };
}

/**
 * User-facing hint appended to every cover-sync banner. Surfaced in one place
 * so wording can evolve without touching every banner branch.
 */
const COVER_SYNC_SETTINGS_HINT = 'Check Game artwork sources and SteamGridDB settings.';

/** User-visible banner when some automatic cover downloads failed.
 *
 * Returns `null` when `failures` is empty so callers (typically piped through
 * `combineCoverSyncMessages`) can suppress the banner without an explicit
 * branch. Without this guard, accessing `failures[0].title` on an empty array
 * threw `TypeError: Cannot read properties of undefined (reading 'title')`,
 * which propagated up to `startMissingCoverSync` and surfaced as a generic
 * "Background cover sync failed" toast even when every cover had downloaded.
 */
export function formatCoverSyncBanner(failures: readonly CoverFetchFailure[]): string | null {
  if (failures.length === 0) {
    return null;
  }

  const first = failures[0];

  if (failures.length === 1) {
    return `Could not download a cover for ${first.title}: ${first.message}. ${COVER_SYNC_SETTINGS_HINT}`;
  }

  const summary = `${first.title}: ${first.message}`;

  return `Could not download covers for ${failures.length} games. First failure: ${summary}. ${COVER_SYNC_SETTINGS_HINT}`;
}

/** Merges background sync banner text with a post-sync refresh error, if any. */
export function combineCoverSyncMessages(
  syncBanner: string | null,
  refreshAfterSyncError: string | null,
): string | null {
  const messages = [syncBanner, refreshAfterSyncError].filter(isDefined);

  return messages.length > 0 ? messages.join(' ') : null;
}
