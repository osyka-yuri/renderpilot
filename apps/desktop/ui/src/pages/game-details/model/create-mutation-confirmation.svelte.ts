import type { MutationSafetyTokens } from '@entities/addon';
import type { D3d12ExecutableMutationAction } from '@shared/model';
import { t } from '@shared/i18n';
import { publishErrorNotification } from '@shared/notifications';
import {
  persistGeneralFileSafetyWarningOptOut,
  shouldShowGeneralFileSafetyWarning,
} from './file-safety-notice-session';
import { createFileSafetyNotice, type FileSafetyNotice } from './file-safety-notice-policy';
import type {
  CapturedFileSafetyContext,
  FileSafetyScope,
} from './create-file-safety-context.svelte';

export type MutationConfirmationRequest = {
  scope: FileSafetyScope;
  actions?: readonly D3d12ExecutableMutationAction[];
  isCurrent?: () => boolean;
};

export type PendingMutationConfirmation = {
  notice: FileSafetyNotice | null;
  actions: D3d12ExecutableMutationAction[];
  isCurrent: () => boolean;
  resolve: (accepted: boolean, rememberGeneralWarning?: boolean) => void;
};

type Options = {
  getGameId: () => string | null;
  getInstallPath: () => string | null;
  captureFreshContext: (scope: FileSafetyScope) => Promise<CapturedFileSafetyContext>;
  isCurrentCapture: (captured: CapturedFileSafetyContext) => boolean;
  invalidatePendingCapture: () => void;
};

/** Owns the single human confirmation that precedes a file-changing action. */
export function createMutationConfirmationCoordinator(options: Options) {
  let pending = $state.raw<PendingMutationConfirmation | null>(null);
  let requesting = $state(false);
  let destroyed = false;
  let requestGeneration = 0;
  let cancelCurrentPreparation: (() => void) | null = null;
  let observedIdentity = false;
  let lastGameId: string | null = null;
  let lastInstallPath: string | null = null;

  function isCurrentRequest(
    gameId: string,
    installPath: string,
    captured: CapturedFileSafetyContext,
    request: MutationConfirmationRequest,
  ): boolean {
    return (
      !destroyed &&
      options.getGameId() === gameId &&
      options.getInstallPath() === installPath &&
      options.isCurrentCapture(captured) &&
      (request.isCurrent?.() ?? true)
    );
  }

  async function requestTokens(
    request: MutationConfirmationRequest,
  ): Promise<MutationSafetyTokens | null> {
    if (pending || requesting || destroyed) {
      return null;
    }
    const gameId = options.getGameId();
    const installPath = options.getInstallPath();
    if (!gameId || !installPath) {
      return null;
    }

    const generation = ++requestGeneration;
    requesting = true;
    let cancelPreparation!: () => void;
    const cancelled = new Promise<{ kind: 'cancelled' }>((resolve) => {
      cancelPreparation = () => {
        resolve({ kind: 'cancelled' });
      };
      cancelCurrentPreparation = cancelPreparation;
    });
    try {
      const captureResult = await Promise.race([
        options.captureFreshContext(request.scope).then((captured) => ({
          kind: 'captured' as const,
          captured,
        })),
        cancelled,
      ]);
      if (captureResult.kind === 'cancelled') {
        return null;
      }
      const { captured } = captureResult;
      if (
        generation !== requestGeneration ||
        !isCurrentRequest(gameId, installPath, captured, request)
      ) {
        return null;
      }

      let notice: FileSafetyNotice | null = createFileSafetyNotice(captured.assessment);
      if (notice.kind === 'general') {
        const warningResult = await Promise.race([
          shouldShowGeneralFileSafetyWarning().then((shouldShow) => ({
            kind: 'warning' as const,
            shouldShow,
          })),
          cancelled,
        ]);
        if (warningResult.kind === 'cancelled') {
          return null;
        }
        if (!warningResult.shouldShow) {
          notice = null;
        }
      }

      if (
        generation !== requestGeneration ||
        !isCurrentRequest(gameId, installPath, captured, request)
      ) {
        return null;
      }

      // Every prepared patch/restore is shown to the user. The backend flag
      // controls whether it issued a token, not whether the EXE will change.
      const actions = [...(request.actions ?? [])];
      if (!notice && actions.length === 0) {
        return isCurrentRequest(gameId, installPath, captured, request) ? captured.tokens : null;
      }
      if (!isCurrentRequest(gameId, installPath, captured, request)) {
        return null;
      }

      return await new Promise<MutationSafetyTokens | null>((resolve) => {
        const current: PendingMutationConfirmation = {
          notice,
          actions: [...actions],
          isCurrent: () => isCurrentRequest(gameId, installPath, captured, request),
          resolve: (accepted, rememberGeneralWarning = false) => {
            if (pending !== current) {
              resolve(null);
              return;
            }
            pending = null;
            if (cancelCurrentPreparation === cancelPreparation) {
              cancelCurrentPreparation = null;
            }
            if (accepted && isCurrentRequest(gameId, installPath, captured, request)) {
              if (notice?.kind === 'general' && rememberGeneralWarning) {
                void persistGeneralFileSafetyWarningOptOut().then((saved) => {
                  if (!saved) {
                    publishErrorNotification(
                      t('notify.statusError'),
                      t('gameDetails.fileSafety.preferenceSaveFailed'),
                    );
                  }
                });
              }
              resolve(captured.tokens);
            } else {
              resolve(null);
            }
          },
        };
        pending = current;
      });
    } finally {
      if (cancelCurrentPreparation === cancelPreparation) {
        cancelCurrentPreparation = null;
      }
      if (generation === requestGeneration) {
        requesting = false;
      }
    }
  }

  function cancel(): void {
    requestGeneration += 1;
    const cancelPreparation = cancelCurrentPreparation;
    cancelCurrentPreparation = null;
    if (cancelPreparation) {
      options.invalidatePendingCapture();
      cancelPreparation();
    }
    const current = pending;
    if (current) {
      current.resolve(false);
    }
    pending = null;
    requesting = false;
  }

  $effect(() => {
    const gameId = options.getGameId();
    const installPath = options.getInstallPath();
    if (!observedIdentity) {
      observedIdentity = true;
      lastGameId = gameId;
      lastInstallPath = installPath;
      return;
    }
    if (gameId !== lastGameId || installPath !== lastInstallPath) {
      lastGameId = gameId;
      lastInstallPath = installPath;
      cancel();
      return;
    }

    const current = pending;
    if (current && !current.isCurrent()) {
      cancel();
    }
  });

  function destroy(): void {
    cancel();
    destroyed = true;
  }

  return {
    get pending() {
      return pending;
    },
    get requesting() {
      return requesting;
    },
    requestTokens,
    cancel,
    destroy,
  };
}
