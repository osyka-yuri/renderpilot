import type { AddonMutationResult, MutationSafetyTokens, createAddonStore } from '@entities/addon';

import type { RenoDxApi } from '../api/desktop';
import type { AvailabilityReport, RenoDxInstallState, RenoDxUpdateReport } from './types';

type RenoDxCore = Pick<
  ReturnType<typeof createAddonStore<RenoDxInstallState, RenoDxUpdateReport, AvailabilityReport>>,
  'runBusyMutation'
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
  ) => Promise<MutationSafetyTokens>;
  afterInstallLikeCommit: (gameId: string, token: number) => void | Promise<void>;
};

/** Mutations for the optional DLSS-Fix lifecycle. */
export function createRenoDxDlssFixMutations(options: RenoDxDlssFixMutationOptions) {
  const { api, core, requireSafetyTokens, afterInstallLikeCommit } = options;

  async function installDlssFix(gameId: string): Promise<AddonMutationResult> {
    return core.runBusyMutation(
      gameId,
      async () => {
        const tokens = await requireSafetyTokens?.(gameId, 'game');
        return tokens
          ? api.installDlssFix(gameId, tokens.gameContextToken)
          : api.installDlssFix(gameId);
      },
      {
        errorKey: 'gameDetails.renodx.dlssFixInstallError',
        safetyScope: 'game',
        afterCommit: (token) => afterInstallLikeCommit(gameId, token),
      },
    );
  }

  async function uninstallDlssFix(gameId: string): Promise<AddonMutationResult> {
    return core.runBusyMutation(gameId, () => api.uninstallDlssFix(gameId), {
      errorKey: 'gameDetails.renodx.dlssFixRemoveError',
      clearDownloadProgress: false,
      afterCommit: (token) => afterInstallLikeCommit(gameId, token),
    });
  }

  async function updateDlssFix(gameId: string): Promise<AddonMutationResult> {
    return core.runBusyMutation(
      gameId,
      async () => {
        const tokens = await requireSafetyTokens?.(gameId, 'game');
        return tokens
          ? api.updateDlssFix(gameId, tokens.gameContextToken)
          : api.updateDlssFix(gameId);
      },
      {
        errorKey: 'gameDetails.renodx.dlssFixInstallError',
        safetyScope: 'game',
        afterCommit: (token) => afterInstallLikeCommit(gameId, token),
      },
    );
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
