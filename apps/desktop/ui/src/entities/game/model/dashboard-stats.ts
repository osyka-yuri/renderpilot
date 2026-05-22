import type { GameSummary } from './types';
import { normalizeUpdateCount } from './game-card';

export type DashboardStats = {
  games: number;
  updates: number;
  rollbacksReady: number;
};

export function getDashboardStats(gameCards: readonly GameSummary[]): DashboardStats {
  const stats = createDashboardStats(gameCards.length);

  for (const game of gameCards) {
    stats.updates += getAvailableUpdatesCount(game);
    stats.rollbacksReady += getRollbacksReadyCount(game);
  }

  return stats;
}

function createDashboardStats(games: number): DashboardStats {
  return {
    games,
    updates: 0,
    rollbacksReady: 0,
  };
}

function getAvailableUpdatesCount(game: GameSummary): number {
  if (!game.updates_available) {
    return 0;
  }

  return normalizeUpdateCount(game.update_count);
}

function getRollbacksReadyCount(game: GameSummary): number {
  return game.rollback_available ? 1 : 0;
}
