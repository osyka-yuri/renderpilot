import type { GameCard } from '@shared/api/types';

/** Normalized cache-busting timestamp from the card, or null if absent or invalid. */
export function gameCardCoverUpdatedAtMs(game: GameCard): number | null {
  const value = game.cover_updated_at_ms;

  if (value === undefined || value === null) {
    return null;
  }

  const n = typeof value === 'number' ? value : Number(value);

  return Number.isNaN(n) ? null : n;
}

/** True when the catalog reports persisted cover metadata for this card. */
export function gameCardHasStoredCover(game: GameCard): boolean {
  return gameCardCoverUpdatedAtMs(game) !== null;
}
