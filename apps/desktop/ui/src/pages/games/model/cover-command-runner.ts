import { clearGameCover, fetchGameCover, setGameCover } from '@entities/game';
import { describeCommandError } from '@shared/api';
import {
  publishCoverDownloadedNotification,
  publishCoverOperationErrorNotification,
  publishCoverRemovedNotification,
  publishCoverUpdatedNotification,
  withManualCoverBusy,
} from '@features/cover-ops';
import type { CoverMenuRefs } from './games-page-cover-ops';
import { focusMenuTrigger, selectCoverFilePath } from './games-page-cover-ops';

export type CoverCommandRunner = ReturnType<typeof createCoverCommandRunner>;

export type CoverCommandRunnerDeps = {
  getManualCoverBusyFor: () => string | null;
  setManualCoverBusyFor: (value: string | null) => void;
  getCoverMenuRefs: () => CoverMenuRefs;
  getMenuOpenFor: () => string | null;
  setMenuOpenFor: (value: string | null) => void;
  onClearError: () => void;
  onReloadCards: () => Promise<void>;
};

export function createCoverCommandRunner(deps: CoverCommandRunnerDeps) {
  function closeMenu(): void {
    deps.setMenuOpenFor(null);
  }

  async function runManualCoverCommand(
    gameId: string,
    command: () => Promise<unknown>,
    onSuccess?: () => void,
  ): Promise<void> {
    closeMenu();

    await withManualCoverBusy({
      gameId,
      manualCoverBusyFor: deps.getManualCoverBusyFor(),
      setManualCoverBusyFor: deps.setManualCoverBusyFor,
      task: command,
      onClearError: deps.onClearError,
      onReloadCards: deps.onReloadCards,
      onSuccess,
      onCoverError: publishCoverOperationErrorNotification,
      describeError: describeCommandError,
      focusMenuTrigger: (id) => {
        focusMenuTrigger(deps.getCoverMenuRefs(), id);
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
        focusMenuTrigger(deps.getCoverMenuRefs(), id);
      },
    });

    if (selectedPath === null) {
      return;
    }

    await runManualCoverCommand(
      gameId,
      () => setGameCover(gameId, selectedPath),
      () => {
        publishCoverUpdatedNotification();
      },
    );
  }

  function fetchCover(gameId: string): void {
    void runManualCoverCommand(
      gameId,
      () => fetchGameCover(gameId),
      () => {
        publishCoverDownloadedNotification();
      },
    );
  }

  function clearCover(gameId: string): void {
    void runManualCoverCommand(
      gameId,
      () => clearGameCover(gameId),
      () => {
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
