import {
  getGameFileSafetyAssessment,
  getSharedVulkanSafetyAssessment,
  normalizeSelectableGameId,
  type GameFileSafetyAssessment,
} from '@entities/game';
import type { MutationSafetyTokens } from '@entities/addon';
import { DesktopCommandError } from '@shared/errors';

export type FileSafetyScope = 'game' | 'game_and_shared';

export type CapturedFileSafetyContext = {
  gameId: string;
  assessment: GameFileSafetyAssessment;
  tokens: MutationSafetyTokens;
  installPath: string;
};

type Options = {
  getGameId: () => string | null;
  getInstallPath: () => string | null;
};

/** Owns fresh per-game and shared-Vulkan safety contexts for a game-details page. */
export function createFileSafetyContext(options: Options) {
  let gameAssessment = $state.raw<GameFileSafetyAssessment | null>(null);
  let requestId = 0;
  let destroyed = false;
  let hasObservedInstallation = false;
  let lastGameId: string | null = null;
  let lastInstallPath: string | null = null;
  let gameAssessmentInstallPath: string | null = null;
  type ReloadEntry = {
    gameId: string;
    installPath: string;
    generation: CaptureGeneration;
    promise: Promise<CapturedFileSafetyContext | null>;
  };
  type CaptureGeneration = {
    invalidated: Promise<'invalidated'>;
    invalidate: () => void;
  };

  function createCaptureGeneration(): CaptureGeneration {
    let invalidate!: () => void;
    const invalidated = new Promise<'invalidated'>((resolve) => {
      invalidate = () => {
        resolve('invalidated');
      };
    });
    return { invalidated, invalidate };
  }

  let activeCaptureGeneration = createCaptureGeneration();
  let inFlight: ReloadEntry | null = null;

  function currentGameId(): string | null {
    const rawGameId = options.getGameId();
    if (rawGameId === null) {
      return null;
    }
    const normalizedGameId = normalizeSelectableGameId(rawGameId);
    return normalizedGameId.length > 0 ? normalizedGameId : null;
  }

  function isCurrentInstallation(gameId: string, installPath: string): boolean {
    return currentGameId() === gameId && options.getInstallPath() === installPath;
  }

  function safetyContextError(code: 'safety_context_missing' | 'safety_context_scope_mismatch') {
    return DesktopCommandError.fromDto({ code });
  }

  async function performReload(
    scope: FileSafetyScope,
    gameId: string,
    installPath: string,
  ): Promise<CapturedFileSafetyContext | null> {
    const token = ++requestId;
    try {
      const nextGame = await getGameFileSafetyAssessment(gameId);
      if (token !== requestId || isDestroyed() || !isCurrentInstallation(gameId, installPath)) {
        return null;
      }
      if (normalizeSelectableGameId(nextGame.game_id) !== gameId) {
        throw safetyContextError('safety_context_scope_mismatch');
      }
      let nextShared: Awaited<ReturnType<typeof getSharedVulkanSafetyAssessment>> | null = null;
      if (scope === 'game_and_shared') {
        nextShared = await getSharedVulkanSafetyAssessment();
      }
      if (token !== requestId || isDestroyed() || !isCurrentInstallation(gameId, installPath)) {
        return null;
      }
      gameAssessment = nextGame;
      gameAssessmentInstallPath = installPath;
      return {
        gameId,
        assessment: nextGame,
        installPath,
        tokens: {
          gameContextToken: nextGame.context_token,
          ...(scope === 'game_and_shared' && nextShared
            ? { sharedVulkanContextToken: nextShared.context_token }
            : {}),
        },
      };
    } catch (loadError) {
      if (token !== requestId || isDestroyed() || !isCurrentInstallation(gameId, installPath)) {
        return null;
      }
      throw loadError;
    }
  }

  function beginCapture(scope: FileSafetyScope): ReloadEntry | null {
    const gameId = currentGameId();
    const installPath = options.getInstallPath();
    if (!gameId || !installPath || destroyed) {
      return null;
    }

    const generation = activeCaptureGeneration;
    const current = inFlight;
    const currentMatchesInstallation =
      current?.generation === generation &&
      current.gameId === gameId &&
      current.installPath === installPath;

    // A wider request for the same game must wait for its narrower predecessor,
    // but a newly selected game must never inherit the previous game's latency.
    // Capture generations detach cancelled work; owner checks discard results
    // from a game or installation that is no longer selected.
    const previous = currentMatchesInstallation ? current : null;
    const promise = (async () => {
      // Do not overlap a narrower assessment request with the wider request
      // that follows it. The previous request may fail; the wider request must
      // still get its own chance to establish a complete context.
      if (previous) {
        const predecessor = await Promise.race([
          previous.promise.then(
            () => 'complete' as const,
            () => 'complete' as const,
          ),
          generation.invalidated,
        ]);
        if (predecessor === 'invalidated') {
          return null;
        }
      }
      if (
        generation !== activeCaptureGeneration ||
        !isCurrentInstallation(gameId, installPath) ||
        isDestroyed()
      ) {
        return null;
      }
      const result = await Promise.race([
        performReload(scope, gameId, installPath).then((captured) => ({
          kind: 'complete' as const,
          captured,
        })),
        generation.invalidated.then(() => ({ kind: 'invalidated' as const })),
      ]);
      return result.kind === 'complete' ? result.captured : null;
    })();
    const entry: ReloadEntry = { gameId, installPath, generation, promise };
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

  // Read at the time of each async continuation; TypeScript cannot infer destroy() calls across awaits.
  function isDestroyed(): boolean {
    return destroyed;
  }

  async function captureFreshContext(
    scope: FileSafetyScope = 'game',
  ): Promise<CapturedFileSafetyContext> {
    const gameId = currentGameId();
    const installPath = options.getInstallPath();
    if (!gameId || !installPath || destroyed) {
      throw safetyContextError('safety_context_missing');
    }

    const entry = beginCapture(scope);
    if (!entry) {
      throw safetyContextError('safety_context_missing');
    }
    const captured = await entry.promise;
    if (
      !captured ||
      isDestroyed() ||
      !isCurrentInstallation(gameId, installPath) ||
      captured.installPath !== installPath ||
      captured.assessment.game_id !== gameId
    ) {
      throw safetyContextError('safety_context_scope_mismatch');
    }
    if (!captured.tokens.gameContextToken) {
      throw safetyContextError('safety_context_missing');
    }
    if (scope === 'game_and_shared' && !captured.tokens.sharedVulkanContextToken) {
      throw safetyContextError('safety_context_missing');
    }
    return captured;
  }

  /** Captures assessment and backend authority together for the page coordinator. */
  function isCurrentCapture(captured: CapturedFileSafetyContext): boolean {
    return (
      !isDestroyed() &&
      isCurrentInstallation(captured.gameId, captured.installPath) &&
      gameAssessmentInstallPath === captured.installPath &&
      gameAssessment === captured.assessment &&
      captured.assessment.game_id === captured.gameId
    );
  }

  /** Detaches pending assessments so a cancelled attempt cannot block a retry. */
  function invalidatePendingCapture(): void {
    const invalidatedGeneration = activeCaptureGeneration;
    activeCaptureGeneration = createCaptureGeneration();
    requestId += 1;
    inFlight = null;
    invalidatedGeneration.invalidate();
  }

  $effect(() => {
    const gameId = currentGameId();
    const installPath = options.getInstallPath();
    if (!hasObservedInstallation) {
      hasObservedInstallation = true;
      lastGameId = gameId;
      lastInstallPath = installPath;
      return;
    }
    if (gameId === lastGameId && installPath === lastInstallPath) {
      return;
    }
    lastGameId = gameId;
    lastInstallPath = installPath;
    invalidatePendingCapture();
    gameAssessment = null;
    gameAssessmentInstallPath = null;
  });

  function destroy(): void {
    destroyed = true;
    invalidatePendingCapture();
    gameAssessment = null;
    gameAssessmentInstallPath = null;
  }

  return {
    captureFreshContext,
    isCurrentCapture,
    invalidatePendingCapture,
    destroy,
  };
}
