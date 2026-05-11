import type { GameSummary } from './types';

export function gameCardCoverUpdatedAtMs(game: GameSummary): number | null {
  const value = game.cover_updated_at_ms;

  if (value === undefined || value === null) {
    return null;
  }

  const n = typeof value === 'number' ? value : Number(value);

  return Number.isNaN(n) ? null : n;
}

/** True when the catalog reports persisted cover metadata for this card. */
export function gameCardHasStoredCover(game: GameSummary): boolean {
  return gameCardCoverUpdatedAtMs(game) !== null;
}

export function normalizeUpdateCount(updateCount: number): number {
  if (!Number.isFinite(updateCount)) {
    return 0;
  }

  return Math.max(0, Math.trunc(updateCount));
}
