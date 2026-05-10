import type { GameCard } from '@shared/api/types';
import { gameCoverAssetSrcWithVersion } from '@shared/api/desktop';
import { gameCardCoverUpdatedAtMs, gameCardHasStoredCover } from '@shared/utils/game-card';
import { formatLabel, titleMonogram } from '@shared/utils/presenters';

export type DashboardStats = {
  games: number;
  updates: number;
  backupsReady: number;
};

export type UpdateBadgeTone = 'success' | 'muted';

export type UpdateBadge = {
  label: string;
  tone: UpdateBadgeTone;
};

export type GameCardViewModel = {
  id: string;
  title: string;
  installPath: string;
  monogram: string;
  updateBadge: UpdateBadge;
  libraries: string[];
  coverSrc: string | null;
  hasCover: boolean;
};

export type LibraryFilterOption = {
  value: string;
  label: string;
};

type CoverViewData = Pick<GameCardViewModel, 'coverSrc' | 'hasCover'>;

const UPDATE_BADGE_LABEL = {
  upToDate: 'Up to date',
  genericAvailable: 'Updates available',
} as const;

export function toGameCardViewModel(game: GameCard): GameCardViewModel {
  const cover = getCoverViewData(game);

  return {
    id: game.game_id,
    title: game.title,
    installPath: game.install_path,
    monogram: titleMonogram(game.title),
    updateBadge: getUpdateBadge(game),
    libraries: getLibraryLabels(game),
    coverSrc: cover.coverSrc,
    hasCover: cover.hasCover,
  };
}

export function getDashboardStats(gameCards: readonly GameCard[]): DashboardStats {
  const stats = createDashboardStats(gameCards.length);

  for (const game of gameCards) {
    stats.updates += getAvailableUpdatesCount(game);
    stats.backupsReady += getBackupsReadyCount(game);
  }

  return stats;
}

function createDashboardStats(games: number): DashboardStats {
  return {
    games,
    updates: 0,
    backupsReady: 0,
  };
}

function getCoverViewData(game: GameCard): CoverViewData {
  if (!gameCardHasStoredCover(game)) {
    return createEmptyCoverViewData();
  }

  const updatedAtMs = gameCardCoverUpdatedAtMs(game);

  return {
    hasCover: true,
    coverSrc: updatedAtMs === null ? null : gameCoverAssetSrcWithVersion(game.game_id, updatedAtMs),
  };
}

function createEmptyCoverViewData(): CoverViewData {
  return {
    hasCover: false,
    coverSrc: null,
  };
}

function getLibraryLabels(game: GameCard): string[] {
  return game.library_tags.map(formatLabel);
}

function getUpdateBadge(game: GameCard): UpdateBadge {
  if (!game.updates_available) {
    return {
      label: UPDATE_BADGE_LABEL.upToDate,
      tone: 'muted',
    };
  }

  return {
    label: getAvailableUpdateLabel(game),
    tone: 'success',
  };
}

function getAvailableUpdateLabel(game: GameCard): string {
  const updateCount = getAvailableUpdatesCount(game);

  if (updateCount === 0) {
    return UPDATE_BADGE_LABEL.genericAvailable;
  }

  return `${updateCount} ${getUpdateNoun(updateCount)} available`;
}

function getUpdateNoun(count: number): string {
  return count === 1 ? 'update' : 'updates';
}

function getAvailableUpdatesCount(game: GameCard): number {
  if (!game.updates_available) {
    return 0;
  }

  return normalizeUpdateCount(game.update_count);
}

function getBackupsReadyCount(game: GameCard): number {
  return game.backup_available ? 1 : 0;
}

function normalizeUpdateCount(updateCount: number): number {
  if (!Number.isFinite(updateCount)) {
    return 0;
  }

  return Math.max(0, Math.trunc(updateCount));
}
