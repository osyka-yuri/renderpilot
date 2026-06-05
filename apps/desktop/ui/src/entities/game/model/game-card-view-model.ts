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
  /** Discriminator resolved to localized text in the view layer (keeps i18n reactive). */
  kind: 'up-to-date' | 'updates-available';
  /** Number of available updates; 0 for up-to-date or a generic "updates available" badge. */
  count: number;
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
  isFavorite: boolean;
  isHidden: boolean;
};

type CoverViewData = Pick<GameCardViewModel, 'coverSrc' | 'hasCover'>;

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
    isFavorite: game.is_favorite,
    isHidden: game.is_hidden,
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
      kind: 'up-to-date',
      count: 0,
      variant: 'secondary',
    };
  }

  return {
    kind: 'updates-available',
    count: getAvailableUpdatesCount(game),
    variant: 'default',
  };
}

function getAvailableUpdatesCount(game: GameSummary): number {
  if (!game.updates_available) {
    return 0;
  }

  return normalizeUpdateCount(game.update_count);
}
