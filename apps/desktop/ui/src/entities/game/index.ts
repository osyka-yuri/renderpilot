export { gameCardHasStoredCover } from './model/game-card';

export { type GameCardViewModel, toGameCardViewModel } from './model/game-card-view-model';

export { type DashboardStats, getDashboardStats } from './model/dashboard-stats';

export {
  type LibraryFilterOption,
  normalizeLibraryValues,
  extractAvailableLibrariesFromCards,
  intersectLibraries,
  hasPartialLibrarySelection,
} from './model/library-filters';

export { LAUNCHER_STEAM, LAUNCHER_GOG } from './model/types';

export type {
  GameSummary,
  CoverArtworkResult,
  GameCardsQuery,
  GameCardsResult,
  GameSelectionHandler,
  GameDetails,
  ScanError,
  AutoScanResponse,
  ScanManualFolderResult,
} from './model/types';

export { formatPartialScanWarning } from './model/scan-presenters';

export {
  normalizeSelectableGameId,
  canonicalGameIdentityId,
  findGameSummaryForSelection,
  gameCardExists,
  areSameGameIds,
} from './model/selection';

export {
  DEFAULT_GAME_CARDS_CATALOG_SORT,
  DEFAULT_GAME_CARDS_CATALOG_PAGE,
  normalizeGameCardsQuery,
} from './api/game-cards-query';

export {
  queryGameCards,
  fetchGameCover,
  clearGameCover,
  setGameCover,
  getGameDetails,
} from './api/desktop';
export { default as GameCard } from './ui/GameCard.svelte';
export { default as GameCardCoverMenu } from './ui/GameCardCoverMenu.svelte';
export { default as GamesDashboardSummary } from './ui/GamesDashboardSummary.svelte';
export type { GameCardCoverMenuHandle } from './ui/types';
