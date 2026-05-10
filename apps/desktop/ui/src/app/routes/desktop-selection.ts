/**
 * Pure helpers for deriving the selected game's details, catalog card mirror, and shell title.
 *
 * Desktop shell components use legacy `$: x = fn(...)`. Under Svelte 5, callers must pass
 * reactive inputs explicitly (screen, selected ids, arrays) into these functions inside the
 * reactive statement — do not rely on hidden closure reads-only helpers, or updates can be skipped.
 */

import type { Screen } from '@app/routes/screen';
import { isWorkspaceScreen } from '@app/routes/desktop-app-controller';
import type { GameCard, GameDetails } from '@shared/api/types';

/** Align launcher ids with the catalog backend. Rust trims identifiers too. */
export function normalizeSelectableGameId(value: string): string {
  return value.trim();
}

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

export function canonicalGameIdentityId(details: GameDetails | null): string | null {
  if (details === null) {
    return null;
  }

  return normalizeGameIdentityId(details.game.identity.id);
}

export function findGameCardForSelection(
  selectionId: string | null,
  gameCards: readonly GameCard[],
): GameCard | null {
  const targetId = normalizeNonEmptyGameId(selectionId);

  if (targetId === null) {
    return null;
  }

  return gameCards.find((card) => normalizeSelectableGameId(card.game_id) === targetId) ?? null;
}

export function workspaceShellGameTitle(
  card: GameCard | null,
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

function normalizeNonEmptyGameId(value: string | null): string | null {
  return normalizeNonEmptyText(value);
}

function normalizeNonEmptyText(value: string | null | undefined): string | null {
  const trimmed = value?.trim() ?? '';

  return trimmed.length > 0 ? trimmed : null;
}

function normalizeGameIdentityId(value: unknown): string | null {
  const text = stringifyGameIdentityId(value);

  return normalizeNonEmptyText(text);
}

function stringifyGameIdentityId(value: unknown): string | null {
  switch (typeof value) {
    case 'string':
      return value;

    case 'number':
      return Number.isFinite(value) ? String(value) : null;

    case 'boolean':
      return value ? 'true' : 'false';

    case 'bigint':
      return value.toString();

    default:
      return null;
  }
}
