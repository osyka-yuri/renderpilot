<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';

  import type { GameCard } from '@shared/api/types';
  import type { GameSelectionHandler, VoidHandler } from '@shared/utils/callbacks';
  import {
    clearGameCover,
    fetchGameCover,
    isDesktopPreviewMode,
    setGameCover,
  } from '@shared/api/desktop';
  import { describeCommandError } from '@shared/api/errors';
  import Popover from '@shared/ui/Popover.svelte';
  import type { PopoverOpenChangeEvent } from '@shared/ui/popover-types';
  import { formatLabel } from '@shared/utils/presenters';

  import GamesEmptyState from '@features/games/GamesEmptyState.svelte';
  import GamesHeaderBar from '@features/games/components/GamesHeaderBar.svelte';
  import GamesFilterToolbar from '@features/games/components/GamesFilterToolbar.svelte';
  import GamesGrid from '@features/games/components/GamesGrid.svelte';
  import GamesLibraryFilterPopover from '@features/games/components/GamesLibraryFilterPopover.svelte';
  import type { GameCardCoverMenuHandle } from '@features/games/components/game-card-types';
  import {
    createGamesFilterPersistence,
    type GamesFilterPersistenceContext,
  } from '@features/games/games-filter-persistence';
  import {
    applyDraftFilters,
    cancelFilterPopover as cancelPopoverState,
    createInitialGamesFilterState,
    openFilterPopover as openPopoverState,
    toggleDraftLibrary as toggleDraftLibraryState,
    type GamesFilterState,
    withSearchQuery,
  } from '@features/games/games-filter-state';
  import {
    extractAvailableLibrariesFromCards,
    type PersistedGamesFilters,
  } from '@features/games/games-screen-filters';
  import {
    getDashboardStats,
    type LibraryFilterOption,
    toGameCardViewModel,
  } from '@features/games/games-screen-model';
  import { createGamesScreenQueryScheduler } from '@features/games/games-screen-query-scheduler';
  import {
    hasFilterIndicator as hasFilterIndicatorState,
    isCoverOperationBusy as isCoverOperationBusyState,
    loadPersistedGamesFilters,
    pruneCoverMenuState,
    shouldCloseOpenMenu,
    shouldQueueAvailabilityPersist,
    syncGamesFilterState,
    withManualCoverBusy,
  } from '@features/games/games-screen-controller';

  const SCAN_LABEL = 'Scan Folder';
  const SCANNING_LABEL = 'Scanning...';
  const DESKTOP_APP_REQUIRED_MESSAGE = 'Choosing a cover file requires the desktop app.';

  const COVER_IMAGE_FILTERS = [
    {
      name: 'Images',
      extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'],
    },
  ];

  type CoverMenuRefs = Record<string, GameCardCoverMenuHandle | undefined>;

  const filterPersistence = createGamesFilterPersistence();

  const noop: VoidHandler = (): void => undefined;
  const noopOpenGame: GameSelectionHandler = (): void => undefined;
  const noopReloadCards = (): Promise<void> => Promise.resolve();
  const noopCoverError = (): void => undefined;

  type Props = {
    games?: GameCard[];
    catalogVersion?: number;
    busy?: boolean;
    coversAutoFetchingIds?: ReadonlySet<string>;

    onScan?: VoidHandler;
    onRefresh?: VoidHandler;
    onReloadCards?: () => Promise<void>;
    onClearError?: VoidHandler;
    onCoverError?: (message: string) => void;
    onOpenDetails?: GameSelectionHandler;
    onOpenOperations?: GameSelectionHandler;
  };

  let {
    games = [],
    catalogVersion = 0,
    busy = false,
    coversAutoFetchingIds = new Set(),

    onScan = noop,
    onRefresh = noop,
    onReloadCards = noopReloadCards,
    onClearError = noop,
    onCoverError = noopCoverError,
    onOpenDetails = noopOpenGame,
    onOpenOperations = noopOpenGame,
  }: Props = $props();

  let manualCoverBusyFor = $state<string | null>(null);
  let menuOpenFor = $state<string | null>(null);
  let coverMenuRefs = $state<CoverMenuRefs>({});

  let filtersState = $state(createInitialGamesFilterState());
  let filterPreferenceLoaded = $state(false);
  let persistedFilters = $state<PersistedGamesFilters | null>(null);
  let filtersAnchorRef = $state<HTMLDivElement | null>(null);

  let availabilityPersistSnapshot = $state('');

  const scheduler = createGamesScreenQueryScheduler();

  let queriedGames = $state<GameCard[]>([]);

  const queryAvailableLibraries = $derived(extractAvailableLibrariesFromCards(games));
  const libraryFilterOptions = $derived(queryAvailableLibraries.map(toLibraryFilterOption));
  const gameItems = $derived(queriedGames.map(toGameCardViewModel));

  $effect(() => {
    syncAvailableLibraryFilters(
      filtersState,
      filterPreferenceLoaded,
      persistedFilters,
      queryAvailableLibraries,
      availabilityPersistSnapshot,
    );
  });

  $effect(() => {
    if (shouldCloseOpenMenu(menuOpenFor, manualCoverBusyFor, coversAutoFetchingIds)) {
      menuOpenFor = null;
    }
  });

  $effect(() => {
    pruneCoverMenuRefs(gameItems.map((game) => game.id));
  });

  $effect(() => {
    const querySnapshot = scheduler.createGamesQuerySnapshot(
      catalogVersion,
      filtersState.ready,
      filterPreferenceLoaded,
      filtersState.searchQuery,
      filtersState.appliedLibraries,
    );

    if (querySnapshot !== null && scheduler.canRunGamesQuery(querySnapshot.requestKey)) {
      void scheduler.runGamesQuery(querySnapshot, {
        setItems(next) {
          queriedGames = next;
        },
        getLibraries() {
          return queryAvailableLibraries;
        },
        setLibraries(_next) {
          // Reactively driven by 'games' prop.
        },
      });
    }
  });

  onMount(() => {
    let disposed = false;

    void loadFilterPreferences(() => disposed);

    return () => {
      disposed = true;
      filterPersistence.flushQueuedSearchPersist(createFilterPersistenceContext());
      filterPersistence.dispose();
    };
  });

  function toLibraryFilterOption(value: string): LibraryFilterOption {
    return {
      value,
      label: formatLabel(value),
    };
  }

  function syncAvailableLibraryFilters(
    currentState: GamesFilterState,
    preferenceLoaded: boolean,
    filters: PersistedGamesFilters | null,
    availableLibraries: readonly string[],
    currentPersistSnapshot: string,
  ): void {
    const syncResult = syncGamesFilterState(
      currentState,
      preferenceLoaded,
      filters,
      availableLibraries,
    );

    if (syncResult.state !== currentState) {
      filtersState = syncResult.state;
    }

    if (!syncResult.didAdjustApplied) {
      return;
    }

    queueAvailabilityPersist(syncResult.state, currentPersistSnapshot);
  }

  function queueAvailabilityPersist(
    nextState: GamesFilterState,
    currentPersistSnapshot: string,
  ): void {
    const persistResult = shouldQueueAvailabilityPersist(
      nextState,
      filterPreferenceLoaded,
      currentPersistSnapshot,
    );

    if (!persistResult.shouldQueue) {
      return;
    }

    availabilityPersistSnapshot = persistResult.nextSnapshot;

    void persistFilters()
      .catch((error: unknown) => {
        console.error('Failed to persist adjusted game filters.', error);
      })
      .finally(() => {
        if (availabilityPersistSnapshot === persistResult.nextSnapshot) {
          availabilityPersistSnapshot = '';
        }
      });
  }

  async function loadFilterPreferences(isDisposed: () => boolean): Promise<void> {
    try {
      const value = await loadPersistedGamesFilters();

      if (isDisposed()) {
        return;
      }

      persistedFilters = value;
    } catch (error: unknown) {
      if (!isDisposed()) {
        console.error('Failed to load persisted game filters.', error);
      }
    } finally {
      if (!isDisposed()) {
        filterPreferenceLoaded = true;
      }
    }
  }

  function createFilterPersistenceContext(): GamesFilterPersistenceContext {
    return {
      getState: () => filtersState,
      setState(nextState: GamesFilterState): void {
        filtersState = nextState;
      },
    };
  }

  function persistFilters(): Promise<void> {
    return filterPersistence.persistFilters(createFilterPersistenceContext());
  }

  function openFilterPopover(): void {
    filtersState = openPopoverState(filtersState);
  }

  function handleFilterPopoverOpenChange(event: PopoverOpenChangeEvent): void {
    if (event.open) {
      openFilterPopover();
      return;
    }

    cancelFilterSelection();
  }

  async function applyFilterSelection(): Promise<void> {
    filtersState = applyDraftFilters(filtersState);

    try {
      await persistFilters();
    } catch (error: unknown) {
      console.error('Failed to persist selected game filters.', error);
    }
  }

  function handleApplyFilterSelection(): void {
    void applyFilterSelection();
  }

  function cancelFilterSelection(): void {
    filtersState = cancelPopoverState(filtersState);
  }

  function toggleFiltersPopover(): void {
    handleFilterPopoverOpenChange({
      open: !filtersState.isPopoverOpen,
      reason: 'programmatic',
    });
  }

  function handleDraftLibraryToggle(library: string): void {
    filtersState = toggleDraftLibraryState(filtersState, library);
  }

  function handleSearchValueChange(nextValue: string): void {
    const nextState = withSearchQuery(filtersState, nextValue);

    if (nextState === filtersState) {
      return;
    }

    filtersState = nextState;
    filterPersistence.queueSearchPersist(createFilterPersistenceContext());
  }

  function pruneCoverMenuRefs(activeGameIds: readonly string[]): void {
    const nextState = pruneCoverMenuState(coverMenuRefs, menuOpenFor, activeGameIds);

    coverMenuRefs = nextState.refs;
    menuOpenFor = nextState.menuOpenFor;
  }

  function isCoverOperationBusy(gameId: string): boolean {
    return isCoverOperationBusyState(gameId, manualCoverBusyFor, coversAutoFetchingIds);
  }

  function setMenuOpen(gameId: string, open: boolean): void {
    menuOpenFor = open ? gameId : null;
  }

  function closeMenu(): void {
    menuOpenFor = null;
  }

  function focusMenuTrigger(gameId: string): void {
    const focus = (): void => {
      coverMenuRefs[gameId]?.focusTrigger();
    };

    if (typeof requestAnimationFrame === 'function') {
      requestAnimationFrame(focus);
      return;
    }

    focus();
  }

  async function runCoverCommand(gameId: string, command: () => Promise<unknown>): Promise<void> {
    closeMenu();

    await withManualCoverBusy({
      gameId,
      manualCoverBusyFor,
      setManualCoverBusyFor: (value) => {
        manualCoverBusyFor = value;
      },
      task: command,
      onClearError,
      onReloadCards,
      onCoverError,
      describeError: describeCommandError,
      focusMenuTrigger,
    });
  }

  async function selectCoverFilePath(gameId: string): Promise<string | null> {
    if (isDesktopPreviewMode()) {
      onCoverError(DESKTOP_APP_REQUIRED_MESSAGE);
      focusMenuTrigger(gameId);
      return null;
    }

    try {
      const selectedPath = await open({
        multiple: false,
        filters: COVER_IMAGE_FILTERS,
      });

      if (typeof selectedPath === 'string') {
        return selectedPath;
      }

      focusMenuTrigger(gameId);
      return null;
    } catch (error: unknown) {
      onCoverError(describeCommandError(error));
      focusMenuTrigger(gameId);
      return null;
    }
  }

  async function handlePickCover(gameId: string): Promise<void> {
    closeMenu();

    if (manualCoverBusyFor !== null) {
      return;
    }

    const selectedPath = await selectCoverFilePath(gameId);

    if (selectedPath === null) {
      return;
    }

    await runCoverCommand(gameId, () => setGameCover(gameId, selectedPath));
  }

  function handleFetchCoverAction(gameId: string): void {
    void runCoverCommand(gameId, () => fetchGameCover(gameId));
  }

  function handlePickCoverAction(gameId: string): void {
    void handlePickCover(gameId);
  }

  function handleClearCoverAction(gameId: string): void {
    void runCoverCommand(gameId, () => clearGameCover(gameId));
  }
</script>

<section class="screen-shell" aria-busy={busy}>
  <GamesHeaderBar
    hasGames={games.length > 0}
    {busy}
    scanButtonLabel={busy ? SCANNING_LABEL : SCAN_LABEL}
    dashboardStats={getDashboardStats(games)}
    {onRefresh}
    {onScan}
  />

  {#if games.length === 0}
    <GamesEmptyState
      {busy}
      scanButtonLabel={busy ? SCANNING_LABEL : SCAN_LABEL}
      {onRefresh}
      {onScan}
    />
  {:else}
    <div class="filters-shell">
      <div class="filters-anchor" bind:this={filtersAnchorRef}>
        <GamesFilterToolbar
          searchQuery={filtersState.searchQuery}
          hasFilterIndicator={hasFilterIndicatorState(
            filtersState.searchQuery,
            filtersState.appliedLibraries,
            queryAvailableLibraries,
          )}
          onSearchChange={handleSearchValueChange}
          onToggleFilters={toggleFiltersPopover}
        />

        <Popover
          anchor={filtersAnchorRef}
          referenceElement={filtersAnchorRef}
          panelClassName="filters-popover"
          open={filtersState.isPopoverOpen}
          aria-label="Library filters"
          onOpenChange={handleFilterPopoverOpenChange}
        >
          <GamesLibraryFilterPopover
            {libraryFilterOptions}
            draftLibraries={filtersState.draftLibraries}
            onToggleLibrary={handleDraftLibraryToggle}
            onCancel={cancelFilterSelection}
            onApply={handleApplyFilterSelection}
          />
        </Popover>
      </div>
    </div>

    <GamesGrid
      games={gameItems}
      {busy}
      hasManualCoverAction={manualCoverBusyFor !== null}
      {coversAutoFetchingIds}
      {menuOpenFor}
      {coverMenuRefs}
      {isCoverOperationBusy}
      onMenuOpenChange={setMenuOpen}
      onFetchCover={handleFetchCoverAction}
      onPickCover={handlePickCoverAction}
      onClearCover={handleClearCoverAction}
      {onOpenDetails}
      {onOpenOperations}
    />
  {/if}
</section>

<style>
  .screen-shell {
    display: grid;
    gap: var(--space-4);
  }

  .filters-shell {
    display: grid;
    gap: var(--space-2);
    padding: 0 var(--space-1);
  }

  .filters-anchor {
    position: relative;
    display: inline-flex;
    flex-direction: column;
  }

  :global(.filters-popover) {
    min-width: min(28rem, 90vw);
    max-width: min(34rem, 92vw);
  }

  @media (max-width: 760px) {
    :global(.filters-popover) {
      min-width: min(24rem, 92vw);
    }
  }
</style>
