import type { GameSummary } from '@entities/game';

/** Appends only the first occurrence of game ids not already present in the session. */
export function appendUniqueGameCards(
  current: readonly GameSummary[],
  incoming: readonly GameSummary[],
): readonly GameSummary[] {
  const knownGameIds = new Set(current.map((game) => game.game_id));
  const appended = incoming.filter((game) => {
    if (knownGameIds.has(game.game_id)) {
      return false;
    }
    knownGameIds.add(game.game_id);
    return true;
  });

  return appended.length === 0 ? current : [...current, ...appended];
}
