<script lang="ts">
  import { onMount } from 'svelte';

  import { getDashboardStats, type GameSummary } from '@entities/game';
  import { type VoidHandler } from '@shared/utils';
  import type { GameSelectionHandler } from '@entities/game';
  import { Popover } from '@shared/ui';
  import { GamesEmptyState, GamesGrid } from '@widgets/games-catalog';
  import { GamesHeaderBar } from '@widgets/games-header';
  import { GamesFilterToolbar, GamesLibraryFilterPopover } from '@features/filter-games';
  import { SCAN_LABEL, SCANNING_LABEL } from '../model/games-page-constants';
  import { createGamesPageModel } from '../model/create-games-page-model.svelte';

  const noop: VoidHandler = (): void => undefined;
  const noopOpenGame: GameSelectionHandler = (): void => undefined;
  const noopReloadCards = (): Promise<void> => Promise.resolve();
  const noopCoverError = (): void => undefined;

  type Props = {
    games?: GameSummary[];
    catalogVersion?: number;
    busy?: boolean;
    coversAutoFetchingIds?: ReadonlySet<string>;
    pickCoverDisabled?: boolean;

    onScan?: VoidHandler;
    onRefresh?: VoidHandler;
    onReloadCards?: () => Promise<void>;
    onClearError?: VoidHandler;
    onCoverError?: (message: string) => void;
    onOpenDetails?: GameSelectionHandler;
    onOpenOperations?: GameSelectionHandler;
  };

  const {
    games = [],
    catalogVersion = 0,
    busy = false,
    coversAutoFetchingIds = new Set(),
    pickCoverDisabled = false,

    onScan = noop,
    onRefresh = noop,
    onReloadCards = noopReloadCards,
    onClearError = noop,
    onCoverError = noopCoverError,
    onOpenDetails = noopOpenGame,
    onOpenOperations = noopOpenGame,
  }: Props = $props();

  const model = createGamesPageModel({
    getGames: () => games,
    getCatalogVersion: () => catalogVersion,
    getBusy: () => busy,
    getCoversAutoFetchingIds: () => coversAutoFetchingIds,
    onClearError: () => {
      onClearError();
    },
    onCoverError: (message) => {
      onCoverError(message);
    },
    onReloadCards: () => {
      return onReloadCards();
    },
  });

  onMount(() => {
    let disposed = false;

    void model.loadFilterPreferences(() => disposed);

    return () => {
      disposed = true;
      model.flushSearchPersist();
      model.dispose();
    };
  });
</script>

<section class="grid gap-4" aria-busy={busy}>
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
    <div class="grid gap-2 px-1">
      <div class="relative inline-flex flex-col" bind:this={model.filtersAnchorRef}>
        <GamesFilterToolbar
          searchQuery={model.filtersState.searchQuery}
          hasFilterIndicator={model.hasFilterIndicator}
          onSearchChange={model.setSearchQuery}
          onToggleFilters={model.toggleFiltersPopover}
        />

        <Popover
          anchor={model.filtersAnchorRef}
          referenceElement={model.filtersAnchorRef}
          open={model.filtersState.isPopoverOpen}
          aria-label="Library filters"
          panelClassName="min-w-[min(28rem,90vw)] max-w-[min(34rem,92vw)] max-md:min-w-[min(24rem,92vw)]"
          onOpenChange={model.handlePopoverOpenChange}
        >
          <GamesLibraryFilterPopover
            libraryFilterOptions={model.libraryFilterOptions}
            draftLibraries={model.filtersState.draftLibraries}
            onToggleLibrary={model.handleToggleLibrary}
            onCancel={model.cancelFilterSelection}
            onApply={model.applyFilterSelection}
          />
        </Popover>
      </div>
    </div>

    <GamesGrid
      games={model.gameItems}
      {busy}
      hasManualCoverAction={model.manualCoverBusyFor !== null}
      pickDisabled={pickCoverDisabled}
      {coversAutoFetchingIds}
      menuOpenFor={model.menuOpenFor}
      coverMenuRefs={model.coverMenuRefs}
      isCoverOperationBusy={model.isCoverOperationBusy}
      onMenuOpenChange={model.setMenuOpen}
      onFetchCover={model.fetchCover}
      onPickCover={model.pickCover}
      onClearCover={model.clearCover}
      {onOpenDetails}
      {onOpenOperations}
    />
  {/if}
</section>
