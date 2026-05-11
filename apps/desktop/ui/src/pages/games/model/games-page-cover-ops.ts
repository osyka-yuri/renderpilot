import type { GameCardCoverMenuHandle } from '@entities/game';
import { selectCoverFilePath as selectCoverFilePathImpl } from '@features/cover-ops';
import { DESKTOP_APP_REQUIRED_MESSAGE } from './games-page-constants';

export type CoverMenuRefs = Record<string, GameCardCoverMenuHandle | undefined>;

export type CoverFilePickerDeps = {
  onCoverError: (message: string) => void;
  focusMenuTrigger: (gameId: string) => void;
};

/** Focuses the cover-menu trigger for the given game, using rAF when available. */
export function focusMenuTrigger(refs: CoverMenuRefs, gameId: string): void {
  const focus = (): void => {
    refs[gameId]?.focusTrigger();
  };

  if (typeof requestAnimationFrame === 'function') {
    requestAnimationFrame(focus);
    return;
  }

  focus();
}

/** Opens the system image picker for a manual cover. Returns the chosen path or null. */
export async function selectCoverFilePath(
  gameId: string,
  deps: CoverFilePickerDeps,
): Promise<string | null> {
  return selectCoverFilePathImpl(gameId, {
    previewModeMessage: DESKTOP_APP_REQUIRED_MESSAGE,
    onCoverError: deps.onCoverError,
    focusMenuTrigger: deps.focusMenuTrigger,
  });
}
