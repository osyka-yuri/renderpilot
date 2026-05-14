<script lang="ts">
  import { onMount } from 'svelte';

  import { getDashboardStats, type GameSelectionHandler, type GameSummary } from '@entities/game';
  import type { VoidHandler } from '@shared/callbacks';
  import { Input } from '@shared/ui';
  import { GamesEmptyState, GamesGrid } from '@widgets/games-catalog';
  import { GamesHeaderBar } from '@widgets/games-header';
  import { GamesFilterDialog } from '@features/filter-games';
  import { SCAN_LABEL, SCANNING_LABEL } from '../model/games-page-constants';
  import { createGamesPageModel } from '../model/create-games-page-model.svelte';

  const SEARCH_LABEL = 'Search games';
  const SEARCH_PLACEHOLDER = 'Search games';
  const FILTERS_BUTTON_LABEL = 'Open filters';
  const FILTERS_BUTTON_ACTIVE_LABEL = 'Open filters, filters active';

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
    coversAutoFetchingIds = new Set<string>(),
    pickCoverDisabled = false,

    onScan = () => undefined,
    onRefresh = () => undefined,
    onReloadCards = () => Promise.resolve(),
    onClearError = () => undefined,
    onOpenDetails = () => undefined,
    onOpenOperations = () => undefined,
  }: Props = $props();

  const hasGames = $derived(games.length > 0);
  const scanButtonLabel = $derived(busy ? SCANNING_LABEL : SCAN_LABEL);
  const dashboardStats = $derived(getDashboardStats(games));

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

  const hasManualCoverAction = $derived(model.manualCoverBusyFor !== null);

  onMount(() => {
    let disposed = false;

    void model.loadFilterPreferences(() => disposed);

    return () => {
      disposed = true;

      try {
        model.flushSearchPersist();
      } finally {
        model.dispose();
      }
    };
  });

  function handleSearchInput(event: Event & { currentTarget: HTMLInputElement }): void {
    model.setSearchQuery(event.currentTarget.value);
  }
</script>

<section class="grid gap-4" aria-busy={busy}>
  <GamesHeaderBar {hasGames} {busy} {scanButtonLabel} {dashboardStats} {onRefresh} {onScan} />

  {#if !hasGames}
    <GamesEmptyState {busy} {scanButtonLabel} {onRefresh} {onScan} />
  {:else}
    <div class="grid gap-2 px-1">
      <div class="flex items-center justify-end gap-2 max-md:justify-stretch" role="search">
        <label
          class="block max-w-88 min-w-48 shrink grow basis-88 max-md:max-w-none max-md:min-w-0"
        >
          <span class="sr-only">{SEARCH_LABEL}</span>

          <Input
            type="search"
            placeholder={SEARCH_PLACEHOLDER}
            value={model.filtersState.searchQuery}
            oninput={handleSearchInput}
          />
        </label>

        <GamesFilterDialog
          open={model.filtersState.isDialogOpen}
          onOpenChange={model.handleDialogOpenChange}
          hasFilterIndicator={model.hasFilterIndicator}
          {filtersButtonLabel}
          groupedLibraryFilterOptions={model.groupedLibraryFilterOptions}
          draftLibraries={model.filtersState.draftLibraries}
          onDraftLibrariesChange={model.handleDraftLibrariesChange}
          launcherFilterOptions={model.launcherFilterOptions}
          draftLaunchers={model.filtersState.draftLaunchers}
          onDraftLaunchersChange={model.handleDraftLaunchersChange}
          draftLauncherOrder={model.filtersState.draftLauncherOrder}
          onDraftLauncherOrderChange={model.handleDraftLauncherOrderChange}
          onCancel={model.cancelFilterSelection}
          onApply={model.applyFilterSelection}
        />
      </div>
    </div>

    <GamesGrid
      games={model.gameItems}
      launcherOrder={model.appliedLauncherOrder}
      {busy}
      {hasManualCoverAction}
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
