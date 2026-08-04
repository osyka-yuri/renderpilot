import type {
  GameSummary,
  GameCardMenuHandle,
  CatalogDelta,
  GamesCatalogBootstrap,
  GamesCatalogScrollAnchor,
  GameCardFocusTarget,
} from '@entities/game';
import {
  ALL_KNOWN_LAUNCHERS,
  ALL_ADDON_CAPABILITIES,
  expandLibraryFilterAliases,
  normalizeAddonCapabilities,
  toGameCardViewModel,
  setGameFavorite,
  setGameHidden,
} from '@entities/game';
import { ALL_KNOWN_LIBRARIES } from '@shared/graphics';
import { createGamesFiltersModel } from '@features/filter-games';
import {
  isCoverOperationBusy as isCoverOperationBusyState,
  pruneCoverMenuState,
  shouldCloseOpenMenu,
} from '@features/cover-ops';
import { publishNotification } from '@shared/notifications';
import { getLocale, t } from '@shared/i18n';
import { reportClientError } from '@shared/errors';
import { SvelteMap } from 'svelte/reactivity';
import { createGamesPageQueryScheduler } from './games-page-query-scheduler';
import { createCoverCommandRunner } from './cover-command-runner';
import { GamesCatalogSessionState } from './games-catalog-session.svelte';
import { createOptimisticBooleanMutationQueue } from './optimistic-boolean-mutation-queue';

export type GamesPageModelInput = {
  getCoversAutoFetchingIds: () => ReadonlySet<string>;
  getOnClearError: () => () => void;
  setFavorite?: (gameId: string, isFavorite: boolean) => Promise<unknown>;
  setHidden?: (gameId: string, isHidden: boolean) => Promise<unknown>;
};

export type GamesCatalogSession = ReturnType<typeof createGamesPageModel>;

export function createGamesPageModel(input: GamesPageModelInput) {
  let manualCoverBusyFor = $state<string | null>(null);
  let menuOpenFor = $state<string | null>(null);
  let actionMenuRefs = $state<Record<string, GameCardMenuHandle | undefined>>({});

  const catalogSession = new GamesCatalogSessionState();
  let debouncedSearchQuery = $state('');
  let scrollTop = 0;
  let scrollAnchor = $state<GamesCatalogScrollAnchor | null>(null);
  let focusedGameId = $state<string | null>(null);
  let focusedTarget = $state<GameCardFocusTarget>('details');
  let focusedGameIndex = 0;
  const favoriteMutations = createOptimisticBooleanMutationQueue();
  const hiddenMutations = createOptimisticBooleanMutationQueue();
  const scheduler = createGamesPageQueryScheduler();
  const persistFavorite = input.setFavorite ?? setGameFavorite;
  const persistHidden = input.setHidden ?? setGameHidden;

  const filtersModel = createGamesFiltersModel({
    getAvailableLibraries: () => ALL_KNOWN_LIBRARIES,
    getAvailableAddons: () => ALL_ADDON_CAPABILITIES,
    getAvailableLaunchers: () => ALL_KNOWN_LAUNCHERS,
  });
  const locale = $derived(getLocale());
  const gameItems = $derived(catalogSession.games.map((game) => toGameCardViewModel(game, locale)));

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
    patchCover: (gameId, updatedAtMs) => {
      patchCard(gameId, { cover_updated_at_ms: updatedAtMs });
    },
  });

  $effect(() => {
    const nextSearchQuery = filtersModel.filtersState.searchQuery;
    const timer = window.setTimeout(() => {
      debouncedSearchQuery = nextSearchQuery;
    }, 150);
    return () => {
      window.clearTimeout(timer);
    };
  });

  $effect(() => {
    const filtersReady = filtersModel.filtersState.ready;
    const selectedLibraries = filtersModel.filtersState.appliedLibraries;
    const selectedAddons = normalizeAddonCapabilities(filtersModel.filtersState.appliedAddons);
    const querySnapshot = scheduler.createGamesQuerySnapshot(
      catalogSession.requestVersion,
      filtersReady,
      filtersReady,
      debouncedSearchQuery,
      expandLibraryFilterAliases(selectedLibraries),
      selectedAddons,
      filtersModel.filtersState.appliedLaunchers,
      filtersModel.filtersState.appliedShowHidden,
      filtersModel.filtersState.appliedFavoritesOnly,
      filtersModel.filtersState.appliedLauncherOrder,
    );

    if (querySnapshot !== null && scheduler.canRunGamesQuery(querySnapshot.requestKey)) {
      if (catalogSession.considerReactiveQuery(querySnapshot)) {
        void scheduler.runGamesQuery(querySnapshot, replacementQuerySinks());
      }
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

  $effect(() => {
    const games = catalogSession.games;
    const focused = focusedGameId;
    if (!focused) {
      return;
    }
    const currentIndex = games.findIndex((game) => game.game_id === focused);
    if (currentIndex >= 0) {
      focusedGameIndex = currentIndex;
      return;
    }
    if (games.length === 0) {
      focusedGameId = null;
      return;
    }
    focusedGameId = games[Math.min(focusedGameIndex, games.length - 1)].game_id;
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
    const previousIndex = catalogSession.games.findIndex((game) => game.game_id === gameId);
    const previousCard = previousIndex >= 0 ? catalogSession.games[previousIndex] : null;
    const requestKey = catalogSession.currentQuery?.requestKey ?? null;
    const { token, mutation } = favoriteMutations.begin(
      gameId,
      previousCard,
      previousIndex,
      requestKey,
      previousCard?.is_favorite,
      isFavorite,
    );
    patchCard(gameId, { is_favorite: isFavorite });
    const changesMembership = filtersModel.filtersState.appliedFavoritesOnly && !isFavorite;
    if (changesMembership) {
      catalogSession.removeCard(gameId);
    } else {
      reorderCurrentCards();
    }
    const persistence = favoriteMutations.enqueue(gameId, () =>
      persistFavorite(gameId, isFavorite),
    );
    try {
      await persistence;
      if (mutation) {
        mutation.confirmedValue = isFavorite;
      }
      if (
        favoriteMutations.isLatest(gameId, token) &&
        (changesMembership || catalogSession.nextOffset !== null)
      ) {
        void refreshCatalog();
      }
      publishNotification({
        severity: 'success',
        title: isFavorite ? t('notify.favoriteAdded') : t('notify.favoriteRemoved'),
      });
    } catch (error) {
      if (favoriteMutations.isLatest(gameId, token)) {
        rollbackOptimisticCardField(
          gameId,
          mutation?.card ?? previousCard,
          mutation?.index ?? previousIndex,
          'is_favorite',
          isFavorite,
          mutation?.confirmedValue ?? previousCard?.is_favorite ?? isFavorite,
          mutation?.requestKey ?? requestKey,
        );
      }
      publishNotification({ severity: 'error', title: t('notify.favoriteFailed') });
      reportClientError('toggle_game_favorite', error);
    } finally {
      favoriteMutations.finish(gameId, persistence);
    }
  }

  async function toggleHidden(gameId: string, isHidden: boolean): Promise<void> {
    const previousIndex = catalogSession.games.findIndex((game) => game.game_id === gameId);
    const previousCard = previousIndex >= 0 ? catalogSession.games[previousIndex] : null;
    const requestKey = catalogSession.currentQuery?.requestKey ?? null;
    const { token, mutation, previousOptimisticValue } = hiddenMutations.begin(
      gameId,
      previousCard,
      previousIndex,
      requestKey,
      previousCard?.is_hidden,
      isHidden,
    );
    patchCard(gameId, { is_hidden: isHidden });
    if (previousOptimisticValue !== undefined && previousOptimisticValue !== isHidden) {
      catalogSession.hiddenCount = Math.max(0, catalogSession.hiddenCount + (isHidden ? 1 : -1));
    }
    const changesMembership = isHidden && !filtersModel.filtersState.appliedShowHidden;
    if (changesMembership) {
      catalogSession.removeCard(gameId);
    }
    const persistence = hiddenMutations.enqueue(gameId, () => persistHidden(gameId, isHidden));
    try {
      await persistence;
      if (mutation) {
        mutation.confirmedValue = isHidden;
      }
      if (
        hiddenMutations.isLatest(gameId, token) &&
        (changesMembership || catalogSession.nextOffset !== null)
      ) {
        void refreshCatalog();
      }
      publishNotification({
        severity: 'success',
        title: isHidden ? t('notify.gameHidden') : t('notify.gameUnhidden'),
      });
    } catch (error) {
      const confirmedValue = mutation?.confirmedValue ?? previousCard?.is_hidden ?? isHidden;
      if (
        hiddenMutations.isLatest(gameId, token) &&
        rollbackOptimisticCardField(
          gameId,
          mutation?.card ?? previousCard,
          mutation?.index ?? previousIndex,
          'is_hidden',
          isHidden,
          confirmedValue,
          mutation?.requestKey ?? requestKey,
        )
      ) {
        catalogSession.hiddenCount = Math.max(
          0,
          catalogSession.hiddenCount + Number(confirmedValue) - Number(isHidden),
        );
      }
      publishNotification({ severity: 'error', title: t('notify.hiddenFailed') });
      reportClientError('toggle_game_hidden', error);
    } finally {
      hiddenMutations.finish(gameId, persistence);
    }
  }

  function rollbackOptimisticCardField<Field extends 'is_favorite' | 'is_hidden'>(
    gameId: string,
    previousCard: GameSummary | null,
    previousIndex: number,
    field: Field,
    optimisticValue: GameSummary[Field],
    confirmedValue: GameSummary[Field],
    requestKey: string | null,
  ): boolean {
    const currentIndex = catalogSession.games.findIndex((game) => game.game_id === gameId);
    if (currentIndex >= 0) {
      const current = catalogSession.games[currentIndex];
      if (previousCard === null || current[field] !== optimisticValue) {
        return false;
      }
      catalogSession.patchCard(gameId, { [field]: confirmedValue });
      return true;
    }
    if (previousCard === null || (catalogSession.currentQuery?.requestKey ?? null) !== requestKey) {
      return false;
    }
    const restoredCard = { ...previousCard, [field]: confirmedValue };
    if (!cardBelongsToCurrentMembership(restoredCard)) {
      return true;
    }
    catalogSession.insertCard(restoredCard, previousIndex);
    return true;
  }

  function cardBelongsToCurrentMembership(card: GameSummary): boolean {
    const state = filtersModel.filtersState;
    return (
      (!card.is_hidden || state.appliedShowHidden) &&
      (!state.appliedFavoritesOnly || card.is_favorite)
    );
  }

  function patchCard(gameId: string, patch: Partial<GameSummary>): void {
    catalogSession.patchCard(gameId, patch);
  }

  function reorderCurrentCards(): void {
    const launcherRank = new SvelteMap(
      filtersModel.filtersState.appliedLauncherOrder.map((launcher, index) => [launcher, index]),
    );
    catalogSession.sortCards((left, right) => {
      const launcherOrder =
        (launcherRank.get(left.launcher) ?? Number.MAX_SAFE_INTEGER) -
        (launcherRank.get(right.launcher) ?? Number.MAX_SAFE_INTEGER);
      if (launcherOrder !== 0) {
        return launcherOrder;
      }
      if (left.is_favorite !== right.is_favorite) {
        return left.is_favorite ? -1 : 1;
      }
      if (left.title !== right.title) {
        return left.title < right.title ? -1 : 1;
      }
      return left.game_id < right.game_id ? -1 : left.game_id > right.game_id ? 1 : 0;
    });
  }

  function replacementQuerySinks() {
    return {
      setItems(next: GameSummary[]) {
        catalogSession.replaceItems(next);
      },
      setCatalogSize(size: number) {
        catalogSession.catalogSize = size;
      },
      setHiddenCount(count: number) {
        catalogSession.hiddenCount = count;
      },
      setCatalogRevision(revision: number) {
        catalogSession.setCatalogRevision(revision);
      },
      setNextOffset(offset: number | null) {
        catalogSession.nextOffset = offset;
      },
    };
  }

  async function refreshCatalog(): Promise<void> {
    const requestVersion = catalogSession.beginRefresh();
    const state = filtersModel.filtersState;
    const query = scheduler.createGamesQuerySnapshot(
      requestVersion,
      state.ready,
      state.ready,
      debouncedSearchQuery,
      expandLibraryFilterAliases(state.appliedLibraries),
      normalizeAddonCapabilities(state.appliedAddons),
      state.appliedLaunchers,
      state.appliedShowHidden,
      state.appliedFavoritesOnly,
      state.appliedLauncherOrder,
    );
    if (query === null) {
      return;
    }
    catalogSession.currentQuery = query;
    await scheduler.runGamesQuery(query, replacementQuerySinks());
  }

  function patchCover(gameId: string, updatedAtMs: number | null): void {
    patchCard(gameId, { cover_updated_at_ms: updatedAtMs });
    for (const mutations of [favoriteMutations, hiddenMutations]) {
      mutations.patchCard(gameId, { cover_updated_at_ms: updatedAtMs });
    }
  }

  function loadNextPage(): void {
    const query = catalogSession.currentQuery;
    const offset = catalogSession.nextOffset;
    if (!query || offset === null) {
      return;
    }
    catalogSession.nextOffset = null;
    const pageQuery = scheduler.createPageQuerySnapshot(
      query,
      offset,
      catalogSession.catalogRevision,
    );
    void scheduler.runGamesQuery(pageQuery, {
      setItems(next) {
        catalogSession.appendItems(next);
      },
      setCatalogSize(size) {
        catalogSession.catalogSize = size;
      },
      setHiddenCount(count) {
        catalogSession.hiddenCount = count;
      },
      setCatalogRevision(revision) {
        catalogSession.setCatalogRevision(revision);
      },
      setNextOffset(value) {
        catalogSession.nextOffset = value;
      },
      onCatalogRevisionMismatch(actualRevision) {
        const restart = scheduler.createRevisionRestartSnapshot(query, actualRevision);
        catalogSession.currentQuery = restart;
        void scheduler.runGamesQuery(restart, replacementQuerySinks());
      },
    });
  }

  function applyBootstrap(bootstrap: GamesCatalogBootstrap): void {
    filtersModel.hydratePreferences(bootstrap.filters);
    catalogSession.applyBootstrap(bootstrap.result);
    debouncedSearchQuery = filtersModel.filtersState.searchQuery;
  }

  function completeBootstrapRecovery(): void {
    catalogSession.completeBootstrapRecovery();
  }

  function completeInitialCatalogSync(): void {
    catalogSession.completeInitialSync();
  }

  function acceptCatalogDelta(delta: CatalogDelta): boolean {
    return catalogSession.acceptDelta(delta);
  }

  return {
    // State
    get manualCoverBusyFor() {
      return manualCoverBusyFor;
    },
    get games() {
      return catalogSession.games;
    },
    get catalogSize() {
      return catalogSession.catalogSize;
    },
    get bootstrapping() {
      return catalogSession.bootstrapping;
    },
    get catalogSyncState() {
      return catalogSession.syncState;
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
      return catalogSession.hiddenCount;
    },
    get catalogRevision() {
      return catalogSession.catalogRevision;
    },
    get pendingCatalogDelta(): CatalogDelta | null {
      return catalogSession.pendingDelta();
    },
    get scrollTop() {
      return scrollTop;
    },
    setScrollTop(value: number) {
      scrollTop = Math.max(0, value);
    },
    get scrollAnchor() {
      return scrollAnchor;
    },
    setScrollAnchor(value: GamesCatalogScrollAnchor) {
      scrollAnchor = {
        gameId: value.gameId,
        offsetWithinRow: value.offsetWithinRow,
      };
    },
    get focusedGameId() {
      return focusedGameId;
    },
    get focusedTarget() {
      return focusedTarget;
    },
    setFocusedGame(gameId: string, target: GameCardFocusTarget = 'details') {
      focusedGameId = gameId;
      focusedTarget = target;
      const index = catalogSession.games.findIndex((game) => game.game_id === gameId);
      if (index >= 0) {
        focusedGameIndex = index;
      }
    },
    loadNextPage,
    refreshCatalog,
    patchCover,
    get hasFilterIndicator() {
      return filtersModel.hasFilterIndicator;
    },

    // Lifecycle
    loadFilterPreferences: filtersModel.loadPreferences,
    applyBootstrap,
    acceptCatalogDelta,
    completeBootstrapRecovery,
    completeInitialCatalogSync,
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
    handleDraftAddonsChange: filtersModel.handleDraftAddonsChange,
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
