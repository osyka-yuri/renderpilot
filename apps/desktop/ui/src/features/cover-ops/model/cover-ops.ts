import { openFilePicker, type DialogFilter } from '@shared/api';

import { publishCoverPickerPreviewModeNotification } from './notifications';

export type CoverMenuRefs<T> = Record<string, T | undefined>;

export type PrunedCoverMenuState<T> = {
  refs: CoverMenuRefs<T>;
  menuOpenFor: string | null;
};

export type ManualCoverBusyParams = {
  gameId: string;
  manualCoverBusyFor: string | null;
  setManualCoverBusyFor: (gameId: string | null) => void;
  task: () => Promise<unknown>;
  onClearError: () => void;
  onReloadCards: () => Promise<void>;
  onSuccess?: () => void;
  onCoverError: (message: string) => void;
  describeError: (error: unknown) => string;
  focusMenuTrigger: (gameId: string) => void;
};

export async function withManualCoverBusy({
  gameId,
  manualCoverBusyFor,
  setManualCoverBusyFor,
  task,
  onClearError,
  onReloadCards,
  onSuccess,
  onCoverError,
  describeError,
  focusMenuTrigger,
}: ManualCoverBusyParams): Promise<void> {
  if (manualCoverBusyFor !== null) {
    return;
  }

  setManualCoverBusyFor(gameId);

  try {
    await task();

    onClearError();
    await onReloadCards();
    onSuccess?.();
  } catch (error: unknown) {
    onCoverError(describeError(error));
  } finally {
    setManualCoverBusyFor(null);
    focusMenuTrigger(gameId);
  }
}

export function isCoverOperationBusy(
  gameId: string,
  manualCoverBusyFor: string | null,
  coversAutoFetchingIds: ReadonlySet<string>,
): boolean {
  return manualCoverBusyFor === gameId || coversAutoFetchingIds.has(gameId);
}

export function shouldCloseOpenMenu(
  menuOpenFor: string | null,
  manualCoverBusyFor: string | null,
  coversAutoFetchingIds: ReadonlySet<string>,
): boolean {
  if (menuOpenFor === null) {
    return false;
  }

  const hasManualCoverOperation = manualCoverBusyFor !== null;
  const hasAutoCoverOperationForOpenMenu = coversAutoFetchingIds.has(menuOpenFor);

  return hasManualCoverOperation || hasAutoCoverOperationForOpenMenu;
}

export function pruneCoverMenuState<T>(
  refs: CoverMenuRefs<T>,
  menuOpenFor: string | null,
  activeGameIds: readonly string[],
): PrunedCoverMenuState<T> {
  const activeGameIdsSet = new Set(activeGameIds);

  let didPruneRefs = false;
  const nextRefs: CoverMenuRefs<T> = {};

  for (const [gameId, menuRef] of Object.entries(refs)) {
    if (activeGameIdsSet.has(gameId)) {
      nextRefs[gameId] = menuRef;
      continue;
    }

    didPruneRefs = true;
  }

  const nextMenuOpenFor = isActiveGameId(menuOpenFor, activeGameIdsSet) ? menuOpenFor : null;

  return {
    refs: didPruneRefs ? nextRefs : refs,
    menuOpenFor: nextMenuOpenFor,
  };
}

function isActiveGameId(
  gameId: string | null,
  activeGameIds: ReadonlySet<string>,
): gameId is string {
  return gameId !== null && activeGameIds.has(gameId);
}

const COVER_IMAGE_FILTERS: DialogFilter[] = [
  {
    name: 'Images',
    extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'],
  },
];

type CoverFilePickerDeps = {
  focusMenuTrigger: (gameId: string) => void;
};

/** Opens the system image picker for a manual cover. Returns the chosen path or null. */
export async function selectCoverFilePath(
  gameId: string,
  deps: CoverFilePickerDeps,
): Promise<string | null> {
  let previewModeTriggered: true | undefined;

  const selectedPath = await openFilePicker({
    filters: COVER_IMAGE_FILTERS,
    onPreviewMode: () => {
      previewModeTriggered = true;
      publishCoverPickerPreviewModeNotification();
      deps.focusMenuTrigger(gameId);
    },
  });

  if (selectedPath !== null) {
    return selectedPath;
  }

  if (previewModeTriggered !== true) {
    deps.focusMenuTrigger(gameId);
  }

  return null;
}
