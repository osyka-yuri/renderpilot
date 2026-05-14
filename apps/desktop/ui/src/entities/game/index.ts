export { gameCardHasStoredCover } from './model/game-card';

export { type GameCardViewModel, toGameCardViewModel } from './model/game-card-view-model';

export { type DashboardStats, getDashboardStats } from './model/dashboard-stats';

export {
  normalizeLibraryValues,
  extractAvailableLibrariesFromCards,
  intersectLibraries,
  hasPartialLibrarySelection,
} from './model/library-filters';

export {
  ALL_KNOWN_LAUNCHERS,
  normalizeLauncherValues,
  extractAvailableLaunchersFromCards,
  hasPartialLauncherSelection,
} from './model/launcher-filters';

export { LAUNCHER_DISPLAY_LABELS, getLauncherDisplayLabel } from './model/launcher-labels';

export { LAUNCHER_STEAM, LAUNCHER_GOG } from './model/types';

export type {
  GameSummary,
  Launcher,
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
export { default as GamesDashboardSummary } from './ui/GamesDashboardSummary.svelte';
export type { GameCardMenuHandle } from './ui/types';

export { createTitleId, createActionAriaLabel } from './model/dom-helpers';
