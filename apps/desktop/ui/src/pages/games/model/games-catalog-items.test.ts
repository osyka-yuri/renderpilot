import { describe, expect, it } from 'vitest';

import { createGameSummary } from '@entities/game';

import { appendUniqueGameCards } from './games-catalog-items';

describe('appendUniqueGameCards', () => {
  it('preserves order and drops duplicates across and within pages', () => {
    const first = createGameSummary({ game_id: 'first' });
    const second = createGameSummary({ game_id: 'second' });
    const third = createGameSummary({ game_id: 'third' });

    const result = appendUniqueGameCards([first], [first, second, second, third]);

    expect(result.map((game) => game.game_id)).toEqual(['first', 'second', 'third']);
  });

  it('preserves the existing array when a page contributes no cards', () => {
    const games = [createGameSummary({ game_id: 'first' })];

    expect(appendUniqueGameCards(games, games)).toBe(games);
  });
});
