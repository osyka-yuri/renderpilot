import {
  requestMutationSafetyTokens,
  type AddonMutationResult,
  type MutationSafetyCapture,
  type MutationSafetyTokens,
  type createAddonStore,
} from '@entities/addon';
import { t, type MessageKeyWithoutParams } from '@shared/i18n';
import { publishPresentedErrorNotification } from '@shared/notifications';

import type { RenoDxApi } from '../api/desktop';
import type {
  AvailabilityReport,
  DlssFixAction,
  RenoDxInstallState,
  RenoDxUpdateReport,
} from './types';

type RenoDxCore = Pick<
  ReturnType<typeof createAddonStore<RenoDxInstallState, RenoDxUpdateReport, AvailabilityReport>>,
  'runBusyMutation' | 'busy'
>;

export type RenoDxDlssFixMutationOptions = {
  api: Pick<
    RenoDxApi,
    'installDlssFix' | 'uninstallDlssFix' | 'updateDlssFix' | 'retryDlssFixRecovery'
  >;
  core: RenoDxCore;
  requireSafetyTokens?: (
    gameId: string,
    scope: 'game' | 'game_and_shared',
  ) => Promise<MutationSafetyTokens | null>;
  getPrimaryAction: () => DlssFixAction | null;
  afterInstallLikeCommit: (gameId: string, token: number) => void | Promise<void>;
};

/** Mutations for the optional DLSS-Fix lifecycle. */
export function createRenoDxDlssFixMutations(options: RenoDxDlssFixMutationOptions) {
  const { api, core, requireSafetyTokens, getPrimaryAction, afterInstallLikeCommit } = options;

  function isUpdateOrRepair(action: DlssFixAction | null): boolean {
    return action === 'update' || action === 'repair';
  }

  function captureSafetyTokens(gameId: string): Promise<MutationSafetyCapture> {
    if (core.busy) {
      return Promise.resolve({ kind: 'cancelled' });
    }

    return requestMutationSafetyTokens(requireSafetyTokens, gameId, 'game');
  }

  function handleSafetyCaptureError(
    error: unknown,
    errorKey: MessageKeyWithoutParams,
  ): AddonMutationResult {
    publishPresentedErrorNotification(t(errorKey), error);
    return 'failed';
  }

  function runWithSafetyTokens(
    gameId: string,
    errorKey: MessageKeyWithoutParams,
    run: (tokens?: MutationSafetyTokens) => Promise<AddonMutationResult>,
  ): Promise<AddonMutationResult> {
    if (!requireSafetyTokens) {
      // Preserve the core mutation's same-turn busy claim when no gate exists.
      return run();
    }

    return captureSafetyTokens(gameId).then(
      (safety) => (safety.kind === 'cancelled' ? 'skipped' : run(safety.tokens)),
      (error: unknown) => handleSafetyCaptureError(error, errorKey),
    );
  }

  async function installDlssFix(gameId: string): Promise<AddonMutationResult> {
    if (getPrimaryAction() !== 'install') {
      return 'skipped';
    }
    return runWithSafetyTokens(gameId, 'gameDetails.renodx.dlssFixInstallError', (tokens) => {
      if (getPrimaryAction() !== 'install') {
        return Promise.resolve('skipped');
      }
      return core.runBusyMutation(
        gameId,
        () =>
          tokens ? api.installDlssFix(gameId, tokens.gameContextToken) : api.installDlssFix(gameId),
        {
          errorKey: 'gameDetails.renodx.dlssFixInstallError',
          safetyScope: 'game',
          afterCommit: (token) => afterInstallLikeCommit(gameId, token),
        },
      );
    });
  }

  async function uninstallDlssFix(gameId: string): Promise<AddonMutationResult> {
    return core.runBusyMutation(gameId, () => api.uninstallDlssFix(gameId), {
      errorKey: 'gameDetails.renodx.dlssFixRemoveError',
      clearDownloadProgress: false,
      afterCommit: (token) => afterInstallLikeCommit(gameId, token),
    });
  }

  async function updateDlssFix(gameId: string): Promise<AddonMutationResult> {
    if (!isUpdateOrRepair(getPrimaryAction())) {
      return 'skipped';
    }
    return runWithSafetyTokens(gameId, 'gameDetails.renodx.dlssFixInstallError', (tokens) => {
      if (!isUpdateOrRepair(getPrimaryAction())) {
        return Promise.resolve('skipped');
      }
      return core.runBusyMutation(
        gameId,
        () =>
          tokens ? api.updateDlssFix(gameId, tokens.gameContextToken) : api.updateDlssFix(gameId),
        {
          errorKey: 'gameDetails.renodx.dlssFixInstallError',
          safetyScope: 'game',
          afterCommit: (token) => afterInstallLikeCommit(gameId, token),
        },
      );
    });
  }

  async function retryDlssFixRecovery(gameId: string): Promise<AddonMutationResult> {
    return core.runBusyMutation(gameId, () => api.retryDlssFixRecovery(gameId), {
      errorKey: 'gameDetails.renodx.dlssFixInstallError',
      clearDownloadProgress: false,
      afterCommit: (token) => afterInstallLikeCommit(gameId, token),
    });
  }

  return {
    installDlssFix,
    uninstallDlssFix,
    updateDlssFix,
    retryDlssFixRecovery,
  };
}
