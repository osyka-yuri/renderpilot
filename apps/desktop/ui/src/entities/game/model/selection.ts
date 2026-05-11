import type { GameDetails, GameSummary } from './types';

/** Align launcher ids with the catalog backend. Rust trims identifiers too. */
export function normalizeSelectableGameId(value: string): string {
  return value.trim();
}

export function canonicalGameIdentityId(details: GameDetails | null): string | null {
  if (details === null) {
    return null;
  }

  return normalizeGameIdentityId(details.game.identity.id);
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

export function findGameSummaryForSelection(
  selectionId: string | null,
  gameCards: readonly GameSummary[],
): GameSummary | null {
  const targetId = normalizeNonEmptyGameId(selectionId);

  if (targetId === null) {
    return null;
  }

  return gameCards.find((card) => normalizeSelectableGameId(card.game_id) === targetId) ?? null;
}

/** True when the game id exists in the provided card list. */
export function gameCardExists(gameCards: readonly GameSummary[], gameId: string): boolean {
  return findGameSummaryForSelection(gameId, gameCards) !== null;
}

/** Compares two game identifiers after normalising them. */
export function areSameGameIds(left: string, right: string): boolean {
  return normalizeSelectableGameId(left) === normalizeSelectableGameId(right);
}

function normalizeNonEmptyGameId(value: string | null): string | null {
  return normalizeNonEmptyText(value);
}

function normalizeNonEmptyText(value: string | null | undefined): string | null {
  const trimmed = value?.trim() ?? '';

  return trimmed.length > 0 ? trimmed : null;
}
