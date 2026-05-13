<script lang="ts">
  import FunnelIcon from '@lucide/svelte/icons/funnel';
  import { onMount } from 'svelte';

  import { getDashboardStats, type GameSummary } from '@entities/game';
  import { cn } from '@shared/classnames';
  import type { VoidHandler } from '@shared/callbacks';
  import type { GameSelectionHandler } from '@entities/game';
  import { Input, Popover, PopoverContent, PopoverTrigger, buttonVariants } from '@shared/ui';
  import { GamesEmptyState, GamesGrid } from '@widgets/games-catalog';
  import { GamesHeaderBar } from '@widgets/games-header';
  import { GamesLibraryFilterPopover } from '@features/filter-games';
  import { SCAN_LABEL, SCANNING_LABEL } from '../model/games-page-constants';
  import { createGamesPageModel } from '../model/create-games-page-model.svelte';

  const SEARCH_LABEL = 'Search games';
  const SEARCH_PLACEHOLDER = 'Search games';
  const FILTERS_BUTTON_LABEL = 'Open library filters';
  const FILTERS_BUTTON_ACTIVE_LABEL = 'Open library filters, filters active';

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
    onOpenDetails?: GameSelectionHandler;
    onOpenOperations?: GameSelectionHandler;
  };

  const {
    games = [],
    catalogVersion = 0,
    busy = false,
    coversAutoFetchingIds = new Set(),
    pickCoverDisabled = false,

    onScan = () => undefined,
    onRefresh = () => undefined,
    onReloadCards = () => Promise.resolve(),
    onClearError = () => undefined,
    onOpenDetails = () => undefined,
    onOpenOperations = () => undefined,
  }: Props = $props();

  function handleClearError(): void {
    onClearError();
  }

  function handleReloadCards(): Promise<void> {
    return onReloadCards();
  }

  const model = createGamesPageModel({
    getGames: () => games,
    getCatalogVersion: () => catalogVersion,
    getBusy: () => busy,
    getCoversAutoFetchingIds: () => coversAutoFetchingIds,
    onClearError: handleClearError,
    onReloadCards: handleReloadCards,
  });

  const filtersButtonLabel = $derived(
    model.hasFilterIndicator ? FILTERS_BUTTON_ACTIVE_LABEL : FILTERS_BUTTON_LABEL,
  );

  onMount(() => {
    let disposed = false;

    void model.loadFilterPreferences(() => disposed);

    return () => {
      disposed = true;
      model.flushSearchPersist();
      model.dispose();
    };
  });

  function handleSearchInput(
    event: Event & { currentTarget: EventTarget & HTMLInputElement },
  ): void {
    model.setSearchQuery(event.currentTarget.value);
  }
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
      <div
        class={cn('flex items-center justify-end gap-2', 'max-md:justify-stretch')}
        role="search"
      >
        <label
          class={cn(
            'block max-w-88 min-w-48 shrink grow basis-88',
            'max-md:max-w-none max-md:min-w-0',
          )}
        >
          <span class="sr-only">{SEARCH_LABEL}</span>
          <Input
            type="search"
            placeholder={SEARCH_PLACEHOLDER}
            value={model.filtersState.searchQuery}
            oninput={handleSearchInput}
          />
        </label>

        <Popover
          open={model.filtersState.isPopoverOpen}
          onOpenChange={model.handlePopoverOpenChange}
        >
          <div class="relative inline-flex flex-none">
            <PopoverTrigger
              class={buttonVariants({ variant: 'secondary', size: 'icon-sm' })}
              aria-label={filtersButtonLabel}
              aria-haspopup="dialog"
            >
              <FunnelIcon class="size-4.5" aria-hidden="true" />
            </PopoverTrigger>

            {#if model.hasFilterIndicator}
              <span
                class={cn(
                  'pointer-events-none absolute -top-0.5 -right-0.5 size-2 rounded-full',
                  'bg-accent ring-2 ring-background',
                )}
                aria-hidden="true"
              ></span>
            {/if}
          </div>

          <PopoverContent align="end" class="w-100">
            <GamesLibraryFilterPopover
              groupedLibraryFilterOptions={model.groupedLibraryFilterOptions}
              draftLibraries={model.filtersState.draftLibraries}
              onDraftLibrariesChange={model.handleDraftLibrariesChange}
              onCancel={model.cancelFilterSelection}
              onApply={model.applyFilterSelection}
            />
          </PopoverContent>
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
