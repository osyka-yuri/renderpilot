import type { GameCard, Nullable } from '@shared/api/types';

type CoverUpdatedAtValue = Nullable<number> | undefined;

/**
 * A valid cover timestamp must be a finite, safe, non-negative integer.
 *
 * We intentionally reject:
 * - NaN
 * - Infinity / -Infinity
 * - negative numbers
 * - fractional numbers
 * - unsafe integers
 */
function isValidCoverUpdatedAtMs(value: number): boolean {
  return Number.isSafeInteger(value) && value >= 0;
}

/**
 * Normalizes the raw cover timestamp value from the API.
 *
 * Returns:
 * - number for a valid persisted cover timestamp
 * - null when the value is absent or invalid
 */
function normalizeCoverUpdatedAtMs(value: CoverUpdatedAtValue): number | null {
  if (value === null || value === undefined) {
    return null;
  }

  return isValidCoverUpdatedAtMs(value) ? value : null;
}

/** Normalized cache-busting timestamp from the card, or null if absent or invalid. */
export function gameCardCoverUpdatedAtMs(game: GameCard): number | null {
  return normalizeCoverUpdatedAtMs(game.cover_updated_at_ms);
}

/** True when the catalog reports valid persisted cover metadata for this card. */
export function gameCardHasStoredCover(game: GameCard): boolean {
  return gameCardCoverUpdatedAtMs(game) !== null;
}
