import {
  requestMutationSafetyTokens,
  type AddonMutationResult,
  type MutationSafetyCapture,
  type MutationSafetyTokens,
} from '@entities/addon';
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
    requireSafetyTokens?: (gameId: string, scope: 'game') => Promise<MutationSafetyTokens | null>;
  } = {},
) {
  const api = options.api ?? optiscalerApi;
  let report = $state<OptiScalerAvailability | null>(null);
  let loading = $state(false);
  let busy = $state(false);
  let checkingUpdates = $state(false);
  let loadError = $state<string | null>(null);
  let safetyContextError = $state<unknown>(null);
  let safetyAttemptGeneration = 0;
  let requestId = 0;
  let mutationSequence = 0;
  let activeMutation: number | null = null;
  let loadedGameId: string | null = null;

  function beginSafetyAttempt(): number {
    safetyContextError = null;
    return ++safetyAttemptGeneration;
  }

  function recordSafetyContextError(owner: number, error: unknown): void {
    if (owner === safetyAttemptGeneration) {
      safetyContextError = isFileSafetyContextError(error) ? error : null;
    }
  }

  async function load(gameId: string): Promise<void> {
    // Stores live for the lifetime of the game-details page. Reset local
    // presentation state when navigation switches games.
    if (loadedGameId !== gameId) {
      loadedGameId = gameId;
      report = null;
      safetyContextError = null;
      safetyAttemptGeneration += 1;
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
    safetyAttempt: number;
  }): Promise<AddonMutationResult> {
    if (activeMutation !== null) {
      return 'skipped';
    }
    const mutationOwner = ++mutationSequence;
    activeMutation = mutationOwner;
    const mutationToken = ++requestId;
    busy = true;
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
        recordSafetyContextError(run.safetyAttempt, error);
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

  function captureSafetyTokens(gameId: string): Promise<MutationSafetyCapture> {
    if (activeMutation !== null) {
      return Promise.resolve({ kind: 'cancelled' });
    }

    return requestMutationSafetyTokens(options.requireSafetyTokens, gameId, 'game');
  }

  function handleSafetyCaptureError(
    error: unknown,
    errorKey: MessageKeyWithoutParams,
    safetyAttempt: number,
  ): AddonMutationResult {
    if (safetyAttempt !== safetyAttemptGeneration) {
      return 'skipped';
    }
    publishPresentedErrorNotification(t(errorKey), error);
    recordSafetyContextError(safetyAttempt, error);
    return 'failed';
  }

  function runWithSafetyTokens(
    gameId: string,
    errorKey: MessageKeyWithoutParams,
    safetyAttempt: number,
    run: (tokens?: MutationSafetyTokens) => Promise<AddonMutationResult>,
  ): Promise<AddonMutationResult> {
    if (!options.requireSafetyTokens) {
      return run();
    }

    return captureSafetyTokens(gameId).then(
      (safety) => {
        if (safetyAttempt !== safetyAttemptGeneration || safety.kind === 'cancelled') {
          return 'skipped';
        }
        return run(safety.tokens);
      },
      (error: unknown) => handleSafetyCaptureError(error, errorKey, safetyAttempt),
    );
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
    safetyAttemptGeneration += 1;
  }

  async function install(gameId: string, modules: string[]): Promise<AddonMutationResult> {
    const safetyAttempt = beginSafetyAttempt();
    const runInstall = (tokens?: MutationSafetyTokens): Promise<AddonMutationResult> =>
      runMutation({
        gameId,
        errorKey: 'gameDetails.optiscaler.installError',
        safetyAttempt,
        action: () =>
          tokens
            ? api.install(gameId, modules, tokens.gameContextToken)
            : api.install(gameId, modules),
        reload: true,
        invalidatePeers: true,
      });

    return runWithSafetyTokens(
      gameId,
      'gameDetails.optiscaler.installError',
      safetyAttempt,
      runInstall,
    );
  }

  function isUpdateAvailable(): boolean {
    return Boolean(report?.lifecycle.update_available && optiscalerManagedTargetAvailable(report));
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
      return isUpdateAvailable();
    },
    get repairRequired() {
      return report?.lifecycle.repair_required ?? false;
    },
    load,
    retry: load,
    deactivate,
    checkForUpdates: async (gameId: string) => {
      const safetyAttempt = beginSafetyAttempt();
      if (activeMutation !== null) {
        return 'skipped';
      }
      checkingUpdates = true;
      try {
        return await runMutation({
          gameId,
          errorKey: 'gameDetails.optiscaler.updateError',
          safetyAttempt,
          action: () => api.checkUpdate(gameId),
          reload: true,
          invalidatePeers: false,
        });
      } finally {
        checkingUpdates = false;
      }
    },
    install,
    update: async (gameId: string) => {
      const safetyAttempt = beginSafetyAttempt();
      if (!isUpdateAvailable()) {
        return 'skipped';
      }
      return runWithSafetyTokens(
        gameId,
        'gameDetails.optiscaler.updateError',
        safetyAttempt,
        (tokens) => {
          if (!isUpdateAvailable()) {
            return Promise.resolve('skipped');
          }
          return runMutation({
            gameId,
            errorKey: 'gameDetails.optiscaler.updateError',
            safetyAttempt,
            action: () =>
              tokens ? api.update(gameId, tokens.gameContextToken) : api.update(gameId),
            reload: true,
            invalidatePeers: true,
          });
        },
      );
    },
    repair: async (gameId: string) => {
      const safetyAttempt = beginSafetyAttempt();
      return runWithSafetyTokens(
        gameId,
        'gameDetails.optiscaler.repairError',
        safetyAttempt,
        (tokens) =>
          runMutation({
            gameId,
            errorKey: 'gameDetails.optiscaler.repairError',
            safetyAttempt,
            action: () =>
              tokens ? api.repair(gameId, tokens.gameContextToken) : api.repair(gameId),
            reload: true,
            invalidatePeers: true,
          }),
      );
    },
    setModules: async (gameId: string, modules: string[]) => {
      const safetyAttempt = beginSafetyAttempt();
      return runWithSafetyTokens(
        gameId,
        'gameDetails.optiscaler.modulesError',
        safetyAttempt,
        (tokens) =>
          runMutation({
            gameId,
            errorKey: 'gameDetails.optiscaler.modulesError',
            safetyAttempt,
            action: () =>
              tokens
                ? api.setModules(gameId, modules, tokens.gameContextToken)
                : api.setModules(gameId, modules),
            reload: true,
            invalidatePeers: true,
          }),
      );
    },
    relocate: async (gameId: string, targetExe: string) => {
      const safetyAttempt = beginSafetyAttempt();
      return runWithSafetyTokens(
        gameId,
        'gameDetails.optiscaler.relocateError',
        safetyAttempt,
        (tokens) =>
          runMutation({
            gameId,
            errorKey: 'gameDetails.optiscaler.relocateError',
            safetyAttempt,
            action: () =>
              tokens
                ? api.relocate(gameId, targetExe, tokens.gameContextToken)
                : api.relocate(gameId, targetExe),
            reload: true,
            invalidatePeers: true,
          }),
      );
    },
    uninstall: (gameId: string) => {
      const safetyAttempt = beginSafetyAttempt();
      return runMutation({
        gameId,
        errorKey: 'gameDetails.optiscaler.uninstallError',
        safetyAttempt,
        action: () => api.uninstall(gameId),
        reload: true,
        invalidatePeers: true,
      });
    },
  };
}
