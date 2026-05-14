<script lang="ts">
  import { onMount } from 'svelte';

  import { getDashboardStats, type GameSelectionHandler, type GameSummary } from '@entities/game';
  import type { VoidHandler } from '@shared/callbacks';
  import { Empty, EmptyHeader, EmptyMedia, EmptyTitle, Input, Spinner } from '@shared/ui';
  import { GamesEmptyState, GamesGrid } from '@widgets/games-catalog';
  import { GamesHeaderBar } from '@widgets/games-header';
  import { GamesFilterDialog } from '@features/filter-games';
  import { SCAN_LABEL, SCANNING_LABEL } from '../model/games-page-constants';
  import { createGamesPageModel } from '../model/create-games-page-model.svelte';

  const SEARCH_LABEL = 'Search games';
  const SEARCH_PLACEHOLDER = 'Search games';

  const FILTERS_BUTTON_LABEL = 'Open filters';
  const FILTERS_BUTTON_ACTIVE_LABEL = 'Open filters, filters active';

  const LOADING_GAMES_LABEL = 'Loading games';

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

  const noop: VoidHandler = () => undefined;
  const noopGameSelection: GameSelectionHandler = () => undefined;
  const noopReloadCards = () => Promise.resolve();

  const {
    games = [],
    catalogVersion = 0,
    busy = false,
    coversAutoFetchingIds = new Set<string>(),
    pickCoverDisabled = false,

    onScan = noop,
    onRefresh = noop,
    onReloadCards = noopReloadCards,
    onClearError = noop,
    onOpenDetails = noopGameSelection,
    onOpenOperations = noopGameSelection,
  }: Props = $props();

  const hasGames = $derived(games.length > 0);
  const showEmptyState = $derived(!hasGames && !busy);
  const showInitialBusyState = $derived(!hasGames && busy);

  const scanButtonLabel = $derived(busy ? SCANNING_LABEL : SCAN_LABEL);
  const dashboardStats = $derived(getDashboardStats(games));

  const model = createGamesPageModel({
    getGames: () => games,
    getCatalogVersion: () => catalogVersion,
    getBusy: () => busy,
    getCoversAutoFetchingIds: () => coversAutoFetchingIds,
    getOnClearError: () => onClearError,
    getOnReloadCards: () => onReloadCards,
  });

  const filtersButtonLabel = $derived(
    model.hasFilterIndicator ? FILTERS_BUTTON_ACTIVE_LABEL : FILTERS_BUTTON_LABEL,
  );

  const hasManualCoverAction = $derived(model.manualCoverBusyFor !== null);

  onMount(() => {
    let disposed = false;
    const isDisposed = () => disposed;

    loadFilterPreferencesSafely(isDisposed);

    return () => {
      disposed = true;
      cleanupModelSafely();
    };
  });

  function loadFilterPreferencesSafely(isDisposed: () => boolean): void {
    model.loadFilterPreferences(isDisposed).catch((error: unknown) => {
      if (!isDisposed()) {
        reportLifecycleError('loadFilterPreferences', error);
      }
    });
  }

  function cleanupModelSafely(): void {
    runSafely('flushSearchPersist', () => {
      model.flushSearchPersist();
    });

    runSafely('dispose', () => {
      model.dispose();
    });
  }

  function runSafely(operation: string, callback: () => void): void {
    try {
      callback();
    } catch (error) {
      reportLifecycleError(operation, error);
    }
  }

  function reportLifecycleError(operation: string, error: unknown): void {
    console.error(`[GamesPage] ${operation} failed`, error);
  }

  function handleSearchInput(event: Event & { currentTarget: HTMLInputElement }): void {
    model.setSearchQuery(event.currentTarget.value);
  }
</script>

<section class="flex min-h-0 flex-col gap-4" aria-busy={busy}>
  {#if showEmptyState}
    <div class="flex flex-1 flex-col items-center justify-center">
      <GamesEmptyState {busy} {scanButtonLabel} {onRefresh} {onScan} />
    </div>
  {:else if showInitialBusyState}
    <Empty class="border-0" role="status" aria-live="polite" aria-atomic="true">
      <EmptyHeader>
        <EmptyMedia>
          <Spinner class="size-10" />
        </EmptyMedia>
        <EmptyTitle>{LOADING_GAMES_LABEL}</EmptyTitle>
      </EmptyHeader>
    </Empty>
  {:else}
    <GamesHeaderBar {hasGames} {busy} {scanButtonLabel} {dashboardStats} {onRefresh} {onScan} />

    <div class="grid gap-2 px-1">
      <div
        class="flex items-center justify-end gap-2 max-md:justify-stretch"
        role="search"
        aria-label={SEARCH_LABEL}
      >
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
      onResetFilters={model.resetFilters}
      {onOpenDetails}
      {onOpenOperations}
    />
  {/if}
</section>
