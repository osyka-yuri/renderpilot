import {
  clearGameCover,
  fetchGameCover,
  setGameCover,
  type CoverArtworkResult,
} from '@entities/game';
import { formatPresentedError } from '@shared/error-presentation';
import {
  publishCoverDownloadedNotification,
  publishCoverOperationErrorNotification,
  publishCoverRemovedNotification,
  publishCoverUpdatedNotification,
  withManualCoverBusy,
} from '@features/cover-ops';
import type { ActionMenuRefs } from './games-page-cover-ops';
import { focusMenuTrigger, selectCoverFilePath } from './games-page-cover-ops';

export type CoverCommandRunner = ReturnType<typeof createCoverCommandRunner>;

export type CoverCommandRunnerDeps = {
  getManualCoverBusyFor: () => string | null;
  setManualCoverBusyFor: (value: string | null) => void;
  getActionMenuRefs: () => ActionMenuRefs;
  getMenuOpenFor: () => string | null;
  setMenuOpenFor: (value: string | null) => void;
  getOnClearError: () => () => void;
  patchCover?: (gameId: string, updatedAtMs: number | null) => void;
};

export function createCoverCommandRunner(deps: CoverCommandRunnerDeps) {
  function closeMenu(): void {
    deps.setMenuOpenFor(null);
  }

  async function runManualCoverCommand<TResult>(
    gameId: string,
    command: () => Promise<TResult>,
    onSuccess?: (result: TResult) => void,
  ): Promise<void> {
    closeMenu();

    await withManualCoverBusy({
      gameId,
      manualCoverBusyFor: deps.getManualCoverBusyFor(),
      setManualCoverBusyFor: deps.setManualCoverBusyFor,
      task: command,
      onClearError: deps.getOnClearError(),
      onSuccess,
      onCoverError: publishCoverOperationErrorNotification,
      describeError: formatPresentedError,
      focusMenuTrigger: (id) => {
        focusMenuTrigger(deps.getActionMenuRefs(), id);
      },
    });
  }

  async function pickAndSetCover(gameId: string): Promise<void> {
    closeMenu();

    if (deps.getManualCoverBusyFor() !== null) {
      return;
    }

    const selectedPath = await selectCoverFilePath(gameId, {
      focusMenuTrigger: (id) => {
        focusMenuTrigger(deps.getActionMenuRefs(), id);
      },
    });

    if (selectedPath === null) {
      return;
    }

    await runManualCoverCommand(
      gameId,
      () => setGameCover(gameId, selectedPath),
      (result: CoverArtworkResult) => {
        deps.patchCover?.(gameId, result.updated_at_ms);
        publishCoverUpdatedNotification();
      },
    );
  }

  function fetchCover(gameId: string): void {
    void runManualCoverCommand(
      gameId,
      () => fetchGameCover(gameId),
      (result: CoverArtworkResult) => {
        deps.patchCover?.(gameId, result.updated_at_ms);
        publishCoverDownloadedNotification();
      },
    );
  }

  function clearCover(gameId: string): void {
    void runManualCoverCommand(
      gameId,
      () => clearGameCover(gameId),
      () => {
        deps.patchCover?.(gameId, null);
        publishCoverRemovedNotification();
      },
    );
  }

  function pickCover(gameId: string): void {
    void pickAndSetCover(gameId);
  }

  return {
    fetchCover,
    pickCover,
    clearCover,
  };
}
