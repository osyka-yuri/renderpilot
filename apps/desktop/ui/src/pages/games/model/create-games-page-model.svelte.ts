import type { GameSummary, GameCardMenuHandle } from '@entities/game';
import {
  ALL_KNOWN_LAUNCHERS,
  extractAvailableLibrariesFromCards,
  toGameCardViewModel,
  setGameFavorite,
  setGameHidden,
} from '@entities/game';
import { createGamesFiltersModel } from '@features/filter-games';
import {
  isCoverOperationBusy as isCoverOperationBusyState,
  pruneCoverMenuState,
  shouldCloseOpenMenu,
} from '@features/cover-ops';
import { publishNotification } from '@shared/notifications';
import { t } from '@shared/i18n';
import { createGamesPageQueryScheduler } from './games-page-query-scheduler';
import { createCoverCommandRunner } from './cover-command-runner';

export type GamesPageModelInput = {
  getGames: () => readonly GameSummary[];
  getCatalogVersion: () => number;
  getBusy: () => boolean;
  getCoversAutoFetchingIds: () => ReadonlySet<string>;
  getOnClearError: () => () => void;
  getOnReloadCards: () => () => Promise<void>;
};

export function createGamesPageModel(input: GamesPageModelInput) {
  let manualCoverBusyFor = $state<string | null>(null);
  let menuOpenFor = $state<string | null>(null);
  let actionMenuRefs = $state<Record<string, GameCardMenuHandle | undefined>>({});

  let queriedGames = $state<GameSummary[]>([]);
  let hiddenCount = $state<number>(0);
  const scheduler = createGamesPageQueryScheduler();

  const queryAvailableLibraries = $derived(extractAvailableLibrariesFromCards(input.getGames()));
  const filtersModel = createGamesFiltersModel({
    getAvailableLibraries: () => queryAvailableLibraries,
    getAvailableLaunchers: () => ALL_KNOWN_LAUNCHERS,
  });
  const gameItems = $derived(queriedGames.map((game) => toGameCardViewModel(game)));

  const coverCommandRunner = createCoverCommandRunner({
    getManualCoverBusyFor: () => manualCoverBusyFor,
    setManualCoverBusyFor: (value) => {
      manualCoverBusyFor = value;
    },
    getActionMenuRefs: () => actionMenuRefs,
    getMenuOpenFor: () => menuOpenFor,
    setMenuOpenFor: (value) => {
      menuOpenFor = value;
    },
    getOnClearError: input.getOnClearError,
    getOnReloadCards: input.getOnReloadCards,
  });

  $effect(() => {
    const filtersReady = filtersModel.filtersState.ready;
    const querySnapshot = scheduler.createGamesQuerySnapshot(
      input.getCatalogVersion(),
      filtersReady,
      filtersReady,
      filtersModel.filtersState.searchQuery,
      filtersModel.filtersState.appliedLibraries,
      filtersModel.filtersState.appliedLaunchers,
      filtersModel.filtersState.appliedShowHidden,
      filtersModel.filtersState.appliedFavoritesOnly,
    );

    if (querySnapshot !== null && scheduler.canRunGamesQuery(querySnapshot.requestKey)) {
      void scheduler.runGamesQuery(querySnapshot, {
        setItems(next) {
          queriedGames = next;
        },
        setHiddenCount(count) {
          hiddenCount = count;
        },
      });
    }
  });

  // ---------------------------------------------------------------------------
  // Cover menu effects
  // ---------------------------------------------------------------------------

  $effect(() => {
    if (shouldCloseOpenMenu(menuOpenFor, manualCoverBusyFor, input.getCoversAutoFetchingIds())) {
      menuOpenFor = null;
    }
  });

  $effect(() => {
    pruneCoverMenuRefs(gameItems.map((game) => game.id));
  });

  // ---------------------------------------------------------------------------
  // Cover menu actions
  // ---------------------------------------------------------------------------

  function pruneCoverMenuRefs(activeGameIds: readonly string[]): void {
    const nextState = pruneCoverMenuState(actionMenuRefs, menuOpenFor, activeGameIds);

    actionMenuRefs = nextState.refs;
    menuOpenFor = nextState.menuOpenFor;
  }

  function isCoverOperationBusy(gameId: string): boolean {
    return isCoverOperationBusyState(gameId, manualCoverBusyFor, input.getCoversAutoFetchingIds());
  }

  function setMenuOpen(gameId: string, open: boolean): void {
    menuOpenFor = open ? gameId : null;
  }

  // ---------------------------------------------------------------------------
  // Cleanup
  // ---------------------------------------------------------------------------

  function flushSearchPersist(): void {
    filtersModel.flushSearchPersist();
  }

  function dispose(): void {
    filtersModel.dispose();
  }

  async function toggleFavorite(gameId: string, isFavorite: boolean): Promise<void> {
    try {
      await setGameFavorite(gameId, isFavorite);
      publishNotification({
        severity: 'success',
        title: isFavorite ? t('notify.favoriteAdded') : t('notify.favoriteRemoved'),
      });
      await input.getOnReloadCards()();
    } catch (error) {
      publishNotification({ severity: 'error', title: t('notify.favoriteFailed') });
      console.error('Failed to toggle favorite status', error);
    }
  }

  async function toggleHidden(gameId: string, isHidden: boolean): Promise<void> {
    try {
      await setGameHidden(gameId, isHidden);
      publishNotification({
        severity: 'success',
        title: isHidden ? t('notify.gameHidden') : t('notify.gameUnhidden'),
      });
      await input.getOnReloadCards()();
    } catch (error) {
      publishNotification({ severity: 'error', title: t('notify.hiddenFailed') });
      console.error('Failed to toggle hidden status', error);
    }
  }

  return {
    // State
    get manualCoverBusyFor() {
      return manualCoverBusyFor;
    },
    get menuOpenFor() {
      return menuOpenFor;
    },
    get actionMenuRefs() {
      return actionMenuRefs;
    },
    get filtersState() {
      return filtersModel.filtersState;
    },
    get filtersAnchorRef() {
      return filtersModel.filtersAnchorRef;
    },
    set filtersAnchorRef(v) {
      filtersModel.filtersAnchorRef = v;
    },

    // Derived
    get groupedLibraryFilterOptions() {
      return filtersModel.groupedLibraryFilterOptions;
    },
    get launcherFilterOptions() {
      return filtersModel.launcherFilterOptions;
    },
    get appliedLauncherOrder() {
      return filtersModel.filtersState.appliedLauncherOrder;
    },
    get gameItems() {
      return gameItems;
    },
    get hiddenCount() {
      return hiddenCount;
    },
    get hasFilterIndicator() {
      return filtersModel.hasFilterIndicator;
    },

    // Lifecycle
    loadFilterPreferences: filtersModel.loadPreferences,
    flushSearchPersist,
    dispose,

    // Actions
    setMenuOpen,
    isCoverOperationBusy,
    handleDialogOpenChange: filtersModel.handleDialogOpenChange,
    applyFilterSelection: filtersModel.applyFilterSelection,
    cancelFilterSelection: filtersModel.cancelFilterSelection,
    toggleFiltersDialog: filtersModel.toggleFiltersDialog,
    handleDraftLibrariesChange: filtersModel.handleDraftLibrariesChange,
    handleDraftLaunchersChange: filtersModel.handleDraftLaunchersChange,
    handleDraftLauncherOrderChange: filtersModel.handleDraftLauncherOrderChange,
    resetFilters: filtersModel.resetFilters,
    quickToggleFavoritesOnly: filtersModel.quickToggleFavoritesOnly,
    quickToggleShowHidden: filtersModel.quickToggleShowHidden,
    setSearchQuery: filtersModel.setSearchQuery,
    fetchCover: coverCommandRunner.fetchCover,
    pickCover: coverCommandRunner.pickCover,
    clearCover: coverCommandRunner.clearCover,
    toggleFavorite,
    toggleHidden,
  };
}
