import type { GameSummary } from '@entities/game';

export type GamesCatalogModel = ReturnType<typeof createGamesCatalogModel>;

export function createGamesCatalogModel() {
  let games = $state<GameSummary[]>([]);
  let catalogVersion = $state(0);

  function setGames(nextGames: GameSummary[]): void {
    games = nextGames;
  }

  function incrementCatalogVersion(): void {
    catalogVersion += 1;
  }

  return {
    get games() {
      return games;
    },
    get catalogVersion() {
      return catalogVersion;
    },
    setGames,
    incrementCatalogVersion,
  };
}
