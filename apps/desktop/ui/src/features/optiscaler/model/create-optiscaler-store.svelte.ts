import type { AddonMutationResult, MutationSafetyTokens } from '@entities/addon';
import { formatPresentedError } from '@shared/error-presentation';
import { isFileSafetyContextError, reportClientError } from '@shared/errors';
import { t, type MessageKeyWithoutParams } from '@shared/i18n';
import { publishPresentedErrorNotification } from '@shared/notifications';
import { clearDownloadProgress } from '@shared/lib';

import { optiscalerApi, type OptiScalerApi } from '../api/desktop';
import { optiscalerManagedTargetAvailable } from './presentation';
import type { OptiScalerAvailability, OptiScalerOperationResult } from './types';

export type OptiScalerStore = ReturnType<typeof createOptiScalerStore>;

export function createOptiScalerStore(
  options: {
    api?: OptiScalerApi;
    onAddonStateChange?: (gameId: string) => void;
    onGameDetailsInvalidate?: (gameId: string) => void | Promise<void>;
    requireSafetyTokens?: (gameId: string, scope: 'game') => Promise<MutationSafetyTokens>;
    requireInstallSafetyTokens?: (
      gameId: string,
      scope: 'game',
    ) => Promise<MutationSafetyTokens | null>;
    onSafetyContextError?: (error: unknown, scope: 'game') => void | Promise<void>;
  } = {},
) {
  const api = options.api ?? optiscalerApi;
  let report = $state<OptiScalerAvailability | null>(null);
  let loading = $state(false);
  let busy = $state(false);
  let checkingUpdates = $state(false);
  let loadError = $state<string | null>(null);
  let safetyContextError = $state<unknown>(null);
  let requestId = 0;
  let mutationSequence = 0;
  let activeMutation: number | null = null;
  let loadedGameId: string | null = null;

  async function load(gameId: string): Promise<void> {
    // Stores live for the lifetime of the game-details page. Reset local
    // presentation state when navigation switches games.
    if (loadedGameId !== gameId) {
      loadedGameId = gameId;
      report = null;
      safetyContextError = null;
      checkingUpdates = false;
    }
    const token = ++requestId;
    loading = true;
    loadError = null;
    try {
      const next = await api.availability(gameId);
      if (token !== requestId) {
        return;
      }
      report = next;
    } catch (error) {
      if (token !== requestId) {
        return;
      }
      loadError = formatPresentedError(error);
      publishPresentedErrorNotification(t('addon.availability.loadFailed'), error);
    } finally {
      if (token === requestId) {
        loading = false;
      }
    }
  }

  async function runMutation(run: {
    gameId: string;
    errorKey: MessageKeyWithoutParams;
    action: () => Promise<unknown>;
    reload: boolean;
    invalidatePeers: boolean;
  }): Promise<AddonMutationResult> {
    if (activeMutation !== null) {
      return 'skipped';
    }
    const mutationOwner = ++mutationSequence;
    activeMutation = mutationOwner;
    const mutationToken = ++requestId;
    busy = true;
    safetyContextError = null;
    let backendCommitted = false;
    clearDownloadProgress([run.gameId]);
    try {
      const outcome = await run.action();
      backendCommitted = true;
      const ownsPresentation = mutationToken === requestId && loadedGameId === run.gameId;
      if (
        ownsPresentation &&
        report &&
        outcome &&
        typeof outcome === 'object' &&
        'installed' in outcome
      ) {
        const opResult = outcome as OptiScalerOperationResult;
        report = {
          ...report,
          install: {
            installed: opResult.installed,
            release: opResult.release,
          },
        };
      }
      if (run.reload && ownsPresentation) {
        await load(run.gameId);
      }
      return 'ok';
    } catch (error) {
      if (mutationToken !== requestId) {
        return 'skipped';
      }
      publishPresentedErrorNotification(t(run.errorKey), error);
      if (isFileSafetyContextError(error)) {
        safetyContextError = error;
        void options.onSafetyContextError?.(error, 'game');
      }
      return 'failed';
    } finally {
      if (backendCommitted && run.invalidatePeers) {
        try {
          options.onAddonStateChange?.(run.gameId);
        } catch (error) {
          reportClientError('optiscaler_peer_refresh', error, 'warning');
        }
        try {
          await options.onGameDetailsInvalidate?.(run.gameId);
        } catch (error) {
          reportClientError('optiscaler_details_refresh', error, 'warning');
        }
      }
      if (activeMutation === mutationOwner) {
        activeMutation = null;
        busy = false;
      }
    }
  }

  async function gameSafetyToken(gameId: string): Promise<string | undefined> {
    const tokens = await options.requireSafetyTokens?.(gameId, 'game');
    return tokens?.gameContextToken;
  }

  /** Invalidates in-flight presentation work without cancelling backend work. */
  function deactivate(): void {
    requestId += 1;
    loadedGameId = null;
    report = null;
    loading = false;
    checkingUpdates = false;
    loadError = null;
    safetyContextError = null;
  }

  async function install(gameId: string, modules: string[]): Promise<AddonMutationResult> {
    // Do not introduce an extra scheduling turn when no page safety gate is
    // configured. The mutation must claim its busy slot before another load
    // can invalidate that game's presentation request.
    const installTokens = options.requireInstallSafetyTokens
      ? await options.requireInstallSafetyTokens(gameId, 'game')
      : undefined;
    if (options.requireInstallSafetyTokens && installTokens === null) {
      return 'skipped';
    }
    return runMutation({
      gameId,
      errorKey: 'gameDetails.optiscaler.installError',
      action: async () => {
        const token = installTokens?.gameContextToken ?? (await gameSafetyToken(gameId));
        return token ? api.install(gameId, modules, token) : api.install(gameId, modules);
      },
      reload: true,
      invalidatePeers: true,
    });
  }

  return {
    get report() {
      return report;
    },
    get state() {
      return report?.install.installed ? report.install : null;
    },
    get loading() {
      return loading;
    },
    get loaded() {
      return report !== null;
    },
    get busy() {
      return busy;
    },
    get checkingUpdates() {
      return checkingUpdates;
    },
    get loadError() {
      return loadError;
    },
    get safetyContextError() {
      return safetyContextError;
    },
    get updateAvailable() {
      return Boolean(
        report?.lifecycle.update_available && optiscalerManagedTargetAvailable(report),
      );
    },
    get repairRequired() {
      return report?.lifecycle.repair_required ?? false;
    },
    load,
    retry: load,
    deactivate,
    checkForUpdates: async (gameId: string) => {
      if (activeMutation !== null) {
        return 'skipped';
      }
      checkingUpdates = true;
      try {
        return await runMutation({
          gameId,
          errorKey: 'gameDetails.optiscaler.updateError',
          action: () => api.checkUpdate(gameId),
          reload: true,
          invalidatePeers: false,
        });
      } finally {
        checkingUpdates = false;
      }
    },
    install,
    update: (gameId: string) =>
      runMutation({
        gameId,
        errorKey: 'gameDetails.optiscaler.updateError',
        action: async () => {
          const token = await gameSafetyToken(gameId);
          return token ? api.update(gameId, token) : api.update(gameId);
        },
        reload: true,
        invalidatePeers: true,
      }),
    repair: (gameId: string) =>
      runMutation({
        gameId,
        errorKey: 'gameDetails.optiscaler.repairError',
        action: async () => {
          const token = await gameSafetyToken(gameId);
          return token ? api.repair(gameId, token) : api.repair(gameId);
        },
        reload: true,
        invalidatePeers: true,
      }),
    setModules: (gameId: string, modules: string[]) =>
      runMutation({
        gameId,
        errorKey: 'gameDetails.optiscaler.modulesError',
        action: async () => {
          const token = await gameSafetyToken(gameId);
          return token ? api.setModules(gameId, modules, token) : api.setModules(gameId, modules);
        },
        reload: true,
        invalidatePeers: true,
      }),
    relocate: (gameId: string, targetExe: string) =>
      runMutation({
        gameId,
        errorKey: 'gameDetails.optiscaler.relocateError',
        action: async () => {
          const token = await gameSafetyToken(gameId);
          return token ? api.relocate(gameId, targetExe, token) : api.relocate(gameId, targetExe);
        },
        reload: true,
        invalidatePeers: true,
      }),
    uninstall: (gameId: string) =>
      runMutation({
        gameId,
        errorKey: 'gameDetails.optiscaler.uninstallError',
        action: () => api.uninstall(gameId),
        reload: true,
        invalidatePeers: true,
      }),
  };
}
