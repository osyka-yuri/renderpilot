/**
 * Pure helpers for deriving the selected game's details, catalog card mirror, and shell title.
 *
 * Desktop shell components use legacy `$: x = fn(...)`. Under Svelte 5, callers must pass
 * reactive inputs explicitly (screen, selected ids, arrays) into these functions inside the
 * reactive statement — do not rely on hidden closure reads-only helpers, or updates can be skipped.
 */

import type { Screen } from './screen';
import { isWorkspaceScreen } from './workspace';
import { areSameGameIds, canonicalGameIdentityId, normalizeSelectableGameId } from '@entities/game';
import type { GameDetails, GameSummary } from '@entities/game';

export type ResolveSelectedGameDetailsInput = {
  readonly activeScreen: Screen;
  readonly selectedGameId: string | null;
  readonly currentDetails: GameDetails | null;
};

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
  return value === null ? null : normalizeSelectableGameId(value);
}

function normalizeNonEmptyText(value: string | null | undefined): string | null {
  const trimmed = value?.trim() ?? '';

  return trimmed.length > 0 ? trimmed : null;
}

/** True when the selected id matches the given game id. */
export function isGameSelected(selectedId: string | null, gameId: string): boolean {
  return selectedId !== null && areSameGameIds(selectedId, gameId);
}
