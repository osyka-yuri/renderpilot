import type { GameSummary, GameCardMenuHandle } from '@entities/game';
import {
  ALL_KNOWN_LAUNCHERS,
  extractAvailableLibrariesFromCards,
  toGameCardViewModel,
} from '@entities/game';
import { createGamesFiltersModel } from '@features/filter-games';
import {
  isCoverOperationBusy as isCoverOperationBusyState,
  pruneCoverMenuState,
  shouldCloseOpenMenu,
} from '@features/cover-ops';
import { createGamesPageQueryScheduler } from './games-page-query-scheduler';
import { createCoverCommandRunner } from './cover-command-runner';

export type GamesPageModelInput = {
  getGames: () => readonly GameSummary[];
  getCatalogVersion: () => number;
  getBusy: () => boolean;
  getCoversAutoFetchingIds: () => ReadonlySet<string>;
  onClearError: () => void;
  onReloadCards: () => Promise<void>;
};

export function createGamesPageModel(input: GamesPageModelInput) {
  let manualCoverBusyFor = $state<string | null>(null);
  let menuOpenFor = $state<string | null>(null);
  let coverMenuRefs = $state<Record<string, GameCardMenuHandle | undefined>>({});

  let queriedGames = $state<GameSummary[]>([]);
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
    getCoverMenuRefs: () => coverMenuRefs,
    getMenuOpenFor: () => menuOpenFor,
    setMenuOpenFor: (value) => {
      menuOpenFor = value;
    },
    onClearError: input.onClearError,
    onReloadCards: input.onReloadCards,
  });

  $effect(() => {
    const querySnapshot = scheduler.createGamesQuerySnapshot(
      input.getCatalogVersion(),
      filtersModel.filtersState.ready,
      filtersModel.filtersState.ready,
      filtersModel.filtersState.searchQuery,
      filtersModel.filtersState.appliedLibraries,
      filtersModel.filtersState.appliedLaunchers,
    );

    if (querySnapshot !== null && scheduler.canRunGamesQuery(querySnapshot.requestKey)) {
      void scheduler.runGamesQuery(querySnapshot, {
        setItems(next) {
          queriedGames = next;
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
    const nextState = pruneCoverMenuState(coverMenuRefs, menuOpenFor, activeGameIds);

    coverMenuRefs = nextState.refs;
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

  return {
    // State
    get manualCoverBusyFor() {
      return manualCoverBusyFor;
    },
    get menuOpenFor() {
      return menuOpenFor;
    },
    get coverMenuRefs() {
      return coverMenuRefs;
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
    get gameItems() {
      return gameItems;
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
    setSearchQuery: filtersModel.setSearchQuery,
    fetchCover: coverCommandRunner.fetchCover,
    pickCover: coverCommandRunner.pickCover,
    clearCover: coverCommandRunner.clearCover,
  };
}
