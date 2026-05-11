import { gameCardExists } from '@entities/game';
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

  function clearSelectionIfSelectedGameMissing(selectedGameId: string | null): boolean {
    if (selectedGameId === null || gameCardExists(games, selectedGameId)) {
      return false;
    }

    return true;
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
    clearSelectionIfSelectedGameMissing,
  };
}
