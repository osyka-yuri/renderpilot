import { formatPresentedError } from '@shared/error-presentation';
import { reportClientError } from '@shared/errors';
import { t, type MessageKeyWithoutParams } from '@shared/i18n';
import {
  publishPresentedErrorNotification,
  publishWarningNotification,
} from '@shared/notifications';
import {
  createNvapiProfile,
  deleteNvapiProfile,
  getNvapiProfileStatus,
  moveNvapiProfile,
  type NvapiProfileStatus,
} from '@features/nvapi-settings';

export type NvapiProfileContext = ReturnType<typeof createNvapiProfileContext>;

export function createNvapiProfileContext(
  refreshAfterProfileMutation?: (gameId: string) => void | Promise<void>,
) {
  let status = $state<NvapiProfileStatus | null>(null);
  let loading = $state(false);
  let busy = $state(false);
  let loadError = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let refreshError = $state<string | null>(null);
  let activeGameId: string | null = null;
  let reloadGeneration = 0;
  let sessionGeneration = 0;
  let actionGeneration = 0;

  function clearErrors(): void {
    loadError = null;
    actionError = null;
    refreshError = null;
  }

  function isCurrentReload(gameId: string, generation: number): boolean {
    return activeGameId === gameId && reloadGeneration === generation;
  }

  async function reload(gameId: string): Promise<void> {
    if (activeGameId !== null && activeGameId !== gameId) {
      sessionGeneration += 1;
      status = null;
    }
    const generation = ++reloadGeneration;
    activeGameId = gameId;
    loading = true;
    clearErrors();
    try {
      const next = await getNvapiProfileStatus(gameId);
      if (isCurrentReload(gameId, generation)) {
        status = next;
      }
    } catch (error) {
      if (isCurrentReload(gameId, generation)) {
        status = null;
        loadError = formatPresentedError(error);
      }
    } finally {
      if (isCurrentReload(gameId, generation)) {
        loading = false;
      }
    }
  }

  function clear(): void {
    reloadGeneration += 1;
    sessionGeneration += 1;
    actionGeneration += 1;
    activeGameId = null;
    status = null;
    loading = false;
    busy = false;
    clearErrors();
  }

  async function runMutation(
    gameId: string,
    errorKey: MessageKeyWithoutParams,
    mutation: () => Promise<void>,
  ): Promise<boolean> {
    if (!gameId || activeGameId !== gameId || busy || loading) {
      return false;
    }
    const actionToken = ++actionGeneration;
    const session = sessionGeneration;
    busy = true;
    clearErrors();
    try {
      try {
        await mutation();
      } catch (error) {
        const label = t(errorKey);
        const presentedError = formatPresentedError(error);
        reportClientError('nvapi_profile_action', error);
        publishPresentedErrorNotification(label, error);
        if (sessionGeneration === session) {
          await reload(gameId);
          if (sessionGeneration === session) {
            actionError = presentedError;
          }
        }
        return false;
      }

      if (sessionGeneration === session) {
        await reload(gameId);
        if (sessionGeneration === session) {
          try {
            await refreshAfterProfileMutation?.(gameId);
          } catch (error) {
            // The driver mutation has already succeeded; the game-details refresh
            // failure is a separate warning and does not change the action result.
            if (sessionGeneration === session) {
              refreshError = t('gameDetails.profile.refreshFailed');
              reportClientError('nvapi_profile_refresh', error);
              publishWarningNotification(refreshError, formatPresentedError(error));
            }
          }
        }
      }

      return true;
    } finally {
      if (actionGeneration === actionToken) {
        busy = false;
      }
    }
  }

  async function create(gameId: string): Promise<boolean> {
    return runMutation(gameId, 'gameDetails.profile.createFailed', () =>
      createNvapiProfile(gameId),
    );
  }

  async function remove(gameId: string): Promise<boolean> {
    return runMutation(gameId, 'gameDetails.profile.deleteFailed', () =>
      deleteNvapiProfile(gameId),
    );
  }

  async function move(gameId: string, absolutePath: string): Promise<boolean> {
    return moveTo(gameId, absolutePath, false);
  }

  async function moveTo(
    gameId: string,
    absolutePath: string,
    selectAutomatically: boolean,
  ): Promise<boolean> {
    return runMutation(gameId, 'gameDetails.profile.moveFailed', () =>
      moveNvapiProfile(gameId, absolutePath, selectAutomatically),
    );
  }

  return {
    get status() {
      return status;
    },
    get loading() {
      return loading;
    },
    get busy() {
      return busy;
    },
    get loadError() {
      return loadError;
    },
    get actionError() {
      return actionError;
    },
    get refreshError() {
      return refreshError;
    },
    get ownedBindingPath() {
      return !loading && status?.ownedByThisGame && status.state !== 'pending'
        ? status.bindingPath
        : null;
    },
    reload,
    clear,
    create,
    remove,
    move,
    moveTo,
  };
}
