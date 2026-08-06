/**
 * Pure helpers for deriving the selected game's details, catalog card mirror, and shell title.
 *
 * Desktop shell components use legacy `$: x = fn(...)`. Under Svelte 5, callers must pass
 * reactive inputs explicitly (screen, selected ids, arrays) into these functions inside the
 * reactive statement — do not rely on hidden closure reads-only helpers, or updates can be skipped.
 */

import type { Screen } from './screen';
import { isWorkspaceScreen, type WorkspaceScreen } from './workspace';
import { areSameGameIds, canonicalGameIdentityId, normalizeSelectableGameId } from '@entities/game';
import type { CatalogDelta, GameDetails, GameSummary } from '@entities/game';

export type ResolveSelectedGameDetailsInput = {
  readonly activeScreen: Screen;
  readonly selectedGameId: string | null;
  readonly currentDetails: GameDetails | null;
};

export type SelectedWorkspaceTarget = {
  readonly gameId: string;
  readonly screen: WorkspaceScreen;
};

export type SelectedGameCatalogDeltaAction =
  | { kind: 'none' }
  | { kind: 'clear' }
  | ({ kind: 'reload' } & SelectedWorkspaceTarget);

/**
 * Resolves which `GameDetails` object the workspace should render, or `null` when stale / invalid.
 *
 * Workspace screens can render the current details when no explicit selection exists.
 * Non-workspace screens require an explicit selected id matching the details payload.
 */
export function resolveSelectedGameDetails(
  input: ResolveSelectedGameDetailsInput,
): GameDetails | null {
  const details = input.currentDetails;

  if (details === null) {
    return null;
  }

  const detailsId = canonicalGameIdentityId(details);

  if (detailsId === null) {
    return null;
  }

  const selectedId = normalizeOptionalSelectedGameId(input.selectedGameId);

  if (!isGameDetailsAllowedForScreen(input.activeScreen, selectedId, detailsId)) {
    return null;
  }

  return details;
}

/** Resolves a reloadable selected-game target only while its workspace is visible. */
export function resolveSelectedWorkspaceTarget(
  activeScreen: Screen,
  selectedGameId: string | null,
): SelectedWorkspaceTarget | null {
  if (!isWorkspaceScreen(activeScreen)) {
    return null;
  }
  const gameId = normalizeOptionalSelectedGameId(selectedGameId);
  return gameId === null ? null : { gameId, screen: activeScreen };
}

/** Resolves the visible workspace only when it still shows the requested game. */
export function resolveSelectedWorkspaceTargetForGame(
  activeScreen: Screen,
  selectedGameId: string | null,
  gameId: string,
): SelectedWorkspaceTarget | null {
  const current = resolveSelectedWorkspaceTarget(activeScreen, selectedGameId);
  return current !== null && areSameGameIds(current.gameId, gameId) ? current : null;
}

/**
 * Decides how an accepted catalog delta affects the currently visible game
 * workspace. Removed games clear the selection; changed games reload the same
 * workspace screen so details and catalog projections cannot diverge.
 */
export function resolveSelectedGameCatalogDeltaAction(
  activeScreen: Screen,
  selectedGameId: string | null,
  delta: Pick<CatalogDelta, 'changedGameIds' | 'removedGameIds'>,
): SelectedGameCatalogDeltaAction {
  const target = resolveSelectedWorkspaceTarget(activeScreen, selectedGameId);
  if (!target) {
    return { kind: 'none' };
  }
  if (delta.removedGameIds.some((gameId) => areSameGameIds(target.gameId, gameId))) {
    return { kind: 'clear' };
  }
  if (delta.changedGameIds.some((gameId) => areSameGameIds(target.gameId, gameId))) {
    return { kind: 'reload', ...target };
  }
  return { kind: 'none' };
}

export function workspaceShellGameTitle(
  card: GameSummary | null,
  details: GameDetails | null,
): string | null {
  return normalizeNonEmptyText(card?.title) ?? normalizeNonEmptyText(details?.game.identity.title);
}

function isGameDetailsAllowedForScreen(
  activeScreen: Screen,
  selectedId: string | null,
  detailsId: string,
): boolean {
  if (isWorkspaceScreen(activeScreen)) {
    return selectedId === null || selectedId === detailsId;
  }

  return selectedId === detailsId;
}

function normalizeOptionalSelectedGameId(value: string | null): string | null {
  if (value === null) {
    return null;
  }
  const normalized = normalizeSelectableGameId(value);
  return normalized.length > 0 ? normalized : null;
}

function normalizeNonEmptyText(value: string | null | undefined): string | null {
  const trimmed = value?.trim() ?? '';

  return trimmed.length > 0 ? trimmed : null;
}

/** True when the selected id matches the given game id. */
export function isGameSelected(selectedId: string | null, gameId: string): boolean {
  return selectedId !== null && areSameGameIds(selectedId, gameId);
}
