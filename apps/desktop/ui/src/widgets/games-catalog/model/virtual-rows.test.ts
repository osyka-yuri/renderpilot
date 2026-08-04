import { describe, expect, it } from 'vitest';
import { createGameSummary, toGameCardViewModel } from '@entities/game';
import type { GameCardState, LauncherGroup } from './launcher-groups';
import {
  buildGamesVirtualRows,
  findGameVirtualRowIndex,
  findVisibleGamesAnchor,
  gamesGridColumnCount,
  pairExistingVirtualRows,
  shouldLoadMoreRows,
} from './virtual-rows';

describe('buildGamesVirtualRows', () => {
  it('keeps launcher headers and chunks cards by responsive column count', () => {
    const groups = [group('Steam', ['a', 'b', 'c']), group('Epic', ['d'])];

    const rows = buildGamesVirtualRows(groups, 2);

    expect(rows.map((row) => row.key)).toEqual([
      'header:Steam',
      'cards:Steam:a',
      'cards:Steam:c',
      'header:Epic',
      'cards:Epic:d',
    ]);
    expect(rows.filter((row) => row.kind === 'cards').map((row) => row.cards.length)).toEqual([
      2, 1, 1,
    ]);
  });

  it('uses stable game identity keys across column-count changes', () => {
    const groups = [group('Steam', ['a', 'b', 'c', 'd'])];

    const twoColumns = buildGamesVirtualRows(groups, 2);
    const threeColumns = buildGamesVirtualRows(groups, 3);

    expect(twoColumns[1]?.key).toBe('cards:Steam:a');
    expect(threeColumns[1]?.key).toBe('cards:Steam:a');
  });

  it('bounds materialized row count independently of the virtualizer viewport', () => {
    const cardIds = Array.from({ length: 1_000 }, (_, index) => `game-${index}`);
    const rows = buildGamesVirtualRows([group('Steam', cardIds)], 5);

    expect(rows).toHaveLength(201);
    expect(rows[0]?.kind).toBe('header');
  });

  it('omits stale virtualizer indices after an atomic row replacement', () => {
    const rows = buildGamesVirtualRows([group('Steam', ['a'])], 1);
    const current = { index: 1, key: 'current' };
    const stale = { index: 4, key: 'stale' };

    expect(pairExistingVirtualRows([current, stale], rows)).toEqual([
      { virtualRow: current, row: rows[1] },
    ]);
  });

  it('calculates responsive columns and the load-more boundary defensively', () => {
    expect(gamesGridColumnCount(0)).toBe(1);
    expect(gamesGridColumnCount(668)).toBe(2);
    expect(shouldLoadMoreRows(10, 6)).toBe(false);
    expect(shouldLoadMoreRows(10, 7)).toBe(true);
    expect(shouldLoadMoreRows(0, 0)).toBe(false);
  });

  it('captures and restores a game anchor from measured rows', () => {
    const rows = buildGamesVirtualRows([group('Steam', ['a', 'b', 'c'])], 2);

    expect(
      findVisibleGamesAnchor(
        rows,
        [
          { index: 0, start: 0, end: 30 },
          { index: 1, start: 30, end: 230 },
          { index: 2, start: 230, end: 430 },
        ],
        85,
      ),
    ).toEqual({ gameId: 'a', offsetWithinRow: 55 });
    expect(findGameVirtualRowIndex(rows, 'b')).toBe(1);
    expect(findGameVirtualRowIndex(rows, 'missing')).toBe(-1);
  });
});

function group(launcher: LauncherGroup['launcher'], ids: string[]): LauncherGroup {
  return {
    launcher,
    label: launcher,
    cards: ids.map(card),
  };
}

function card(id: string): GameCardState {
  return {
    id,
    game: toGameCardViewModel(createGameSummary({ game_id: id, title: id }), 'en'),
    isCoverBusy: false,
    isBackgroundCoverFetching: false,
    isMenuDisabled: false,
    isPickDisabled: false,
    isMenuOpen: false,
  };
}
