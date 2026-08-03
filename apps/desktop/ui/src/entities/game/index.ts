export { gameCardHasStoredCover } from './model/game-card';

export { type GameCardViewModel, toGameCardViewModel } from './model/game-card-view-model';

export { type DashboardStats, getDashboardStats } from './model/dashboard-stats';

export {
  expandLibraryFilterAliases,
  normalizeLibraryValues,
  intersectLibraries,
  hasPartialLibrarySelection,
} from './model/library-filters';

export {
  ALL_ADDON_CAPABILITIES,
  addonCapabilityLabel,
  hasPartialAddonSelection,
  isAddonCapability,
  normalizeAddonCapabilities,
} from './model/addon-capabilities';

export {
  ALL_KNOWN_LAUNCHERS,
  normalizeLauncherValues,
  extractAvailableLaunchersFromCards,
  hasPartialLauncherSelection,
} from './model/launcher-filters';

export { LAUNCHER_DISPLAY_LABELS, getLauncherDisplayLabel } from './model/launcher-labels';

export { createGameSummary, createGameDetails } from './model/test-support';

export { LAUNCHER_STEAM, LAUNCHER_GOG } from './model/types';

export type {
  GameSummary,
  AddonCapability,
  Launcher,
  CoverArtworkResult,
  RemoveGameFromCatalogResult,
  GameCardsQuery,
  GameCardsResult,
  EffectiveGamesFilters,
  CatalogDelta,
  CatalogDeltaReason,
  CatalogRevision,
  CatalogSyncState,
  GamesCatalogScrollAnchor,
  GameCardFocusTarget,
  GamesCatalogBootstrap,
  GameSelectionHandler,
  GameDetails,
  GameLibraryComponent,
  GameCandidateGroup,
  GameCandidate,
  CoordinatedCandidateOption,
  D3d12ExecutableStatus,
  AutoScanResponse,
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
  bootstrapGamesCatalog,
  fetchGameCover,
  clearGameCover,
  setGameCover,
  getGameDetails,
  setGameFavorite,
  setGameHidden,
  removeGameFromCatalog,
} from './api/desktop';
export { default as GameCard } from './ui/GameCard.svelte';
export { default as GamesDashboardSummary } from './ui/GamesDashboardSummary.svelte';
export type { GameCardMenuHandle } from './ui/types';

export { createTitleId } from './model/dom-helpers';
