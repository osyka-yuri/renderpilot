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
  technologies: string[];
  coverSrc: string | null;
  hasCover: boolean;
};

type CoverViewData = Pick<GameCardViewModel, 'coverSrc' | 'hasCover'>;

const UPDATE_BADGE_LABEL = {
  upToDate: 'Up to date',
  available: 'Updates available',
} as const;

export function toGameCardViewModel(game: GameCard): GameCardViewModel {
  const cover = getCoverViewData(game);

  return {
    id: game.game_id,
    title: game.title,
    installPath: game.install_path,
    monogram: titleMonogram(game.title),
    updateBadge: getUpdateBadge(game),
    technologies: getTechnologyLabels(game),
    coverSrc: cover.coverSrc,
    hasCover: cover.hasCover,
  };
}

export function getDashboardStats(gameCards: readonly GameCard[]): DashboardStats {
  const stats: DashboardStats = {
    games: gameCards.length,
    updates: 0,
    backupsReady: 0,
  };

  for (const game of gameCards) {
    stats.updates += game.updates_available ? getSafeUpdateCount(game) : 0;
    stats.backupsReady += game.backup_available ? 1 : 0;
  }

  return stats;
}

function getCoverViewData(game: GameCard): CoverViewData {
  const hasCover = gameCardHasStoredCover(game);

  if (!hasCover) {
    return {
      hasCover: false,
      coverSrc: null,
    };
  }

  const versionMs = gameCardCoverUpdatedAtMs(game);

  return {
    hasCover: true,
    coverSrc: versionMs === null ? null : gameCoverAssetSrcWithVersion(game.game_id, versionMs),
  };
}

function getTechnologyLabels(game: GameCard): string[] {
  return game.technology_tags.map(formatLabel);
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
  const updateCount = getSafeUpdateCount(game);

  if (updateCount === 0) {
    return UPDATE_BADGE_LABEL.available;
  }

  return `${updateCount} ${pluralizeUpdate(updateCount)} available`;
}

function pluralizeUpdate(count: number): string {
  return count === 1 ? 'update' : 'updates';
}

function getSafeUpdateCount(game: GameCard): number {
  const { update_count: updateCount } = game;

  if (!Number.isFinite(updateCount)) {
    return 0;
  }

  return Math.max(0, Math.trunc(updateCount));
}
