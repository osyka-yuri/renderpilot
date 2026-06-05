import type { GameCardMenuHandle } from '@entities/game';
import { selectCoverFilePath as selectCoverFilePathImpl } from '@features/cover-ops';

export type ActionMenuRefs = Record<string, GameCardMenuHandle | undefined>;

export type CoverFilePickerDeps = {
  focusMenuTrigger: (gameId: string) => void;
};

/** Focuses the cover-menu trigger for the given game, using rAF when available. */
export function focusMenuTrigger(refs: ActionMenuRefs, gameId: string): void {
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
    focusMenuTrigger: deps.focusMenuTrigger,
  });
}
