import {
  getGameFileSafetyAssessment,
  getSharedVulkanSafetyAssessment,
  normalizeSelectableGameId,
  type GameFileSafetyAssessment,
} from '@entities/game';
import type { MutationSafetyTokens } from '@entities/addon';
import { formatPresentedError } from '@shared/error-presentation';
import { publishPresentedErrorNotification } from '@shared/notifications';
import { t } from '@shared/i18n';
import { DesktopCommandError, isFileSafetyContextError } from '@shared/errors';

export type FileSafetyScope = 'game' | 'game_and_shared';

type Options = {
  getGameId: () => string | null;
};

/** Owns fresh per-game and shared-Vulkan safety contexts for a game-details page. */
export function createFileSafetyContext(options: Options) {
  let gameAssessment = $state<GameFileSafetyAssessment | null>(null);
  let sharedVulkanContextToken = $state<string | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let requestId = 0;
  let destroyed = false;
  let lastGameId: string | null = null;
  type ReloadEntry = {
    gameId: string;
    scope: FileSafetyScope;
    notifyOnError: boolean;
    promise: Promise<void>;
  };
  let inFlight: ReloadEntry | null = null;

  function currentGameId(): string | null {
    const rawGameId = options.getGameId();
    if (rawGameId === null) {
      return null;
    }
    const normalizedGameId = normalizeSelectableGameId(rawGameId);
    return normalizedGameId.length > 0 ? normalizedGameId : null;
  }

  function scopeCovers(available: FileSafetyScope, requested: FileSafetyScope): boolean {
    return available === 'game_and_shared' || available === requested;
  }

  function isCurrentGame(gameId: string): boolean {
    return currentGameId() === gameId;
  }

  function safetyContextError(code: 'safety_context_missing' | 'safety_context_scope_mismatch') {
    return DesktopCommandError.fromDto({ code });
  }

  async function performReload(scope: FileSafetyScope, gameId: string): Promise<void> {
    const token = ++requestId;
    loading = true;
    error = null;
    try {
      const nextGame = await getGameFileSafetyAssessment(gameId);
      if (token !== requestId || isDestroyed() || !isCurrentGame(gameId)) {
        return;
      }
      if (normalizeSelectableGameId(nextGame.game_id) !== gameId) {
        throw safetyContextError('safety_context_scope_mismatch');
      }
      let nextShared: Awaited<ReturnType<typeof getSharedVulkanSafetyAssessment>> | null = null;
      if (scope === 'game_and_shared') {
        nextShared = await getSharedVulkanSafetyAssessment();
      }
      if (token !== requestId || isDestroyed() || !isCurrentGame(gameId)) {
        return;
      }
      gameAssessment = nextGame;
      if (nextShared) {
        sharedVulkanContextToken = nextShared.context_token;
      }
    } catch (loadError) {
      if (token !== requestId || isDestroyed()) {
        return;
      }
      error = formatPresentedError(loadError);
      throw loadError;
    } finally {
      if (token === requestId && !isDestroyed()) {
        loading = false;
      }
    }
  }

  function beginReload(scope: FileSafetyScope, notifyOnError: boolean): ReloadEntry | null {
    const gameId = currentGameId();
    if (!gameId || destroyed) {
      return null;
    }

    const current = inFlight;
    if (current?.gameId === gameId && scopeCovers(current.scope, scope)) {
      // A mutation is the consumer of this request, so its operation-level
      // error notification is the single user-visible notification.
      if (!notifyOnError) {
        current.notifyOnError = false;
      }
      return current;
    }

    const entry = {} as ReloadEntry;
    // A wider request for the same game must wait for its narrower predecessor,
    // but a newly selected game must never inherit the previous game's latency.
    // The request id and current-game checks below already discard late results.
    const previous = current?.gameId === gameId ? current : null;
    entry.gameId = gameId;
    entry.scope = scope;
    entry.notifyOnError = notifyOnError;
    entry.promise = (async () => {
      // Do not overlap a narrower assessment request with the wider request
      // that follows it. The previous request may fail; the wider request must
      // still get its own chance to establish a complete context.
      if (previous) {
        try {
          await previous.promise;
        } catch {
          // The wider request below is the authoritative result.
        }
      }
      if (!isCurrentGame(gameId) || isDestroyed()) {
        return;
      }
      await performReload(scope, gameId);
    })();
    inFlight = entry;
    entry.promise.then(
      () => {
        if (inFlight === entry) {
          inFlight = null;
        }
      },
      () => {
        if (inFlight === entry) {
          inFlight = null;
        }
      },
    );
    return entry;
  }

  function isDestroyed(): boolean {
    return destroyed;
  }

  async function reload(scope: FileSafetyScope = 'game'): Promise<void> {
    const entry = beginReload(scope, true);
    if (!entry) {
      return;
    }
    try {
      await entry.promise;
    } catch (loadError) {
      if (entry.notifyOnError) {
        publishPresentedErrorNotification(t('gameDetails.fileSafety.loadError'), loadError);
      }
    }
  }

  async function requireTokens(scope: FileSafetyScope = 'game'): Promise<MutationSafetyTokens> {
    const gameId = currentGameId();
    if (!gameId || destroyed) {
      throw safetyContextError('safety_context_missing');
    }

    const entry = beginReload(scope, false);
    if (!entry) {
      throw safetyContextError('safety_context_missing');
    }
    await entry.promise;

    if (isDestroyed() || !isCurrentGame(gameId) || gameAssessment?.game_id !== gameId) {
      throw safetyContextError('safety_context_scope_mismatch');
    }
    const gameContextToken = gameAssessment.context_token;
    if (!gameContextToken) {
      throw safetyContextError('safety_context_missing');
    }
    if (scope === 'game_and_shared' && !sharedVulkanContextToken) {
      throw safetyContextError('safety_context_missing');
    }
    return {
      gameContextToken,
      ...(scope === 'game_and_shared' ? { sharedVulkanContextToken } : {}),
    };
  }

  async function refreshForMutationError(errorValue: unknown, scope: FileSafetyScope = 'game') {
    if (!isFileSafetyContextError(errorValue)) {
      return;
    }
    const entry = beginReload(scope, false);
    if (!entry) {
      return;
    }
    try {
      await entry.promise;
    } catch {
      // The original mutation already produced the user-visible error. A
      // refresh failure is diagnostic state, not a second notification.
    }
  }

  $effect(() => {
    const gameId = currentGameId();
    if (gameId === lastGameId) {
      return;
    }
    lastGameId = gameId;
    gameAssessment = null;
    sharedVulkanContextToken = null;
    if (gameId) {
      void reload('game');
    }
  });

  function destroy(): void {
    destroyed = true;
    requestId += 1;
    gameAssessment = null;
    sharedVulkanContextToken = null;
  }

  return {
    get assessment() {
      return gameAssessment;
    },
    get gameContextToken() {
      return gameAssessment?.context_token ?? null;
    },
    get sharedVulkanContextToken() {
      return sharedVulkanContextToken;
    },
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },
    reload,
    requireTokens,
    refreshForMutationError,
    destroy,
  };
}
