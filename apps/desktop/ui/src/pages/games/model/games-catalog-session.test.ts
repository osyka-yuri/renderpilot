import { describe, expect, it } from 'vitest';

import { createGameSummary, type GameCardsResult } from '@entities/game';

import { GamesCatalogSessionState } from './games-catalog-session.svelte';

function result(catalogSize: number): GameCardsResult {
  return {
    items: [createGameSummary({ game_id: 'game' })],
    catalogSize,
    total: 1,
    hiddenCount: 0,
    availableLibraries: [],
    availableLaunchers: [],
    catalogRevision: 2,
    nextOffset: null,
  };
}

describe('GamesCatalogSessionState', () => {
  it('installs bootstrap state atomically and suppresses its matching reactive query once', () => {
    const session = new GamesCatalogSessionState();
    const query = {
      requestKey: 'request',
      searchQuery: '',
      selectedLibraries: [],
      selectedAddons: [],
      selectedLaunchers: [],
      launcherOrder: [],
      showHidden: false,
      favoritesOnly: false,
      pageOffset: 0,
    };

    session.applyBootstrap(result(1));

    expect(session.games.map((game) => game.game_id)).toEqual(['game']);
    expect(session.catalogRevision).toBe(2);
    expect(session.syncState).toBe('ready');
    expect(session.considerReactiveQuery(query)).toBe(false);
    expect(session.considerReactiveQuery(query)).toBe(true);
  });

  it('keeps an empty bootstrap refreshing until startup completes', () => {
    const session = new GamesCatalogSessionState();

    session.applyBootstrap(result(0));
    expect(session.syncState).toBe('refreshing');

    session.completeInitialSync();
    expect(session.syncState).toBe('ready');
  });

  it('sorts and inserts without mutating the borrowed source array', () => {
    const alpha = createGameSummary({ game_id: 'alpha', title: 'Alpha' });
    const bravo = createGameSummary({ game_id: 'bravo', title: 'Bravo' });
    const source: readonly ReturnType<typeof createGameSummary>[] = [bravo, alpha];
    const session = new GamesCatalogSessionState();

    session.replaceItems(source);
    session.sortCards((left, right) => left.title.localeCompare(right.title));

    expect(source.map((game) => game.game_id)).toEqual(['bravo', 'alpha']);
    expect(session.games.map((game) => game.game_id)).toEqual(['alpha', 'bravo']);

    session.insertCard(createGameSummary({ game_id: 'middle' }), 1);
    expect(source.map((game) => game.game_id)).toEqual(['bravo', 'alpha']);
    expect(session.games.map((game) => game.game_id)).toEqual(['alpha', 'middle', 'bravo']);
  });
});
