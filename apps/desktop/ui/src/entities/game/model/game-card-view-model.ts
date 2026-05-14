import type { GameSummary } from './types';
import type { BadgeVariant } from '@shared/ui';
import { gameCoverAssetSrcWithVersion } from './cover-urls';
import {
  gameCardCoverUpdatedAtMs,
  gameCardHasStoredCover,
  normalizeUpdateCount,
} from './game-card';
import { titleMonogram } from './presenters';

export type UpdateBadge = {
  label: string;
  variant: BadgeVariant;
};

export type GameCardViewModel = {
  id: string;
  title: string;
  launcher: string;
  installPath: string;
  monogram: string;
  updateBadge: UpdateBadge;
  libraries: string[];
  coverSrc: string | null;
  hasCover: boolean;
};

type CoverViewData = Pick<GameCardViewModel, 'coverSrc' | 'hasCover'>;

const UPDATE_BADGE_LABEL = {
  upToDate: 'Up to date',
  genericAvailable: 'Updates available',
} as const;

export function toGameCardViewModel(game: GameSummary): GameCardViewModel {
  const cover = getCoverViewData(game);

  return {
    id: game.game_id,
    title: game.title,
    launcher: game.launcher,
    installPath: game.install_path,
    monogram: titleMonogram(game.title),
    updateBadge: getUpdateBadge(game),
    libraries: [...game.library_tags],
    coverSrc: cover.coverSrc,
    hasCover: cover.hasCover,
  };
}

function getCoverViewData(game: GameSummary): CoverViewData {
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

function getUpdateBadge(game: GameSummary): UpdateBadge {
  if (!game.updates_available) {
    return {
      label: UPDATE_BADGE_LABEL.upToDate,
      variant: 'secondary',
    };
  }

  return {
    label: getAvailableUpdateLabel(game),
    variant: 'default',
  };
}

function getAvailableUpdateLabel(game: GameSummary): string {
  const updateCount = getAvailableUpdatesCount(game);

  if (updateCount === 0) {
    return UPDATE_BADGE_LABEL.genericAvailable;
  }

  return `${updateCount} ${getUpdateNoun(updateCount)} available`;
}

function getUpdateNoun(count: number): string {
  return count === 1 ? 'update' : 'updates';
}

function getAvailableUpdatesCount(game: GameSummary): number {
  if (!game.updates_available) {
    return 0;
  }

  return normalizeUpdateCount(game.update_count);
}
