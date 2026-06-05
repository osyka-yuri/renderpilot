<script lang="ts">
  import { onMount } from 'svelte';

  import { getDashboardStats, type GameSelectionHandler, type GameSummary } from '@entities/game';
  import type { VoidHandler } from '@shared/callbacks';
  import {
    Button,
    Empty,
    EmptyHeader,
    EmptyMedia,
    EmptyTitle,
    Input,
    ScrollArea,
    Spinner,
  } from '@shared/ui';
  import { cn } from '@shared/classnames';
  import StarIcon from '@lucide/svelte/icons/star';
  import EyeOffIcon from '@lucide/svelte/icons/eye-off';
  import { GamesEmptyState, GamesGrid } from '@widgets/games-catalog';
  import { GamesHeaderBar } from '@widgets/games-header';
  import { GamesFilterDialog } from '@features/filter-games';
  import { t } from '@shared/i18n';
  import { createGamesPageModel } from '../model/create-games-page-model.svelte';

  type Props = {
    games?: GameSummary[];
    catalogVersion?: number;
    busy?: boolean;
    coversAutoFetchingIds?: ReadonlySet<string>;
    pickCoverDisabled?: boolean;

    onScan?: VoidHandler;
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
    onReloadCards = noopReloadCards,
    onClearError = noop,
    onOpenDetails = noopGameSelection,
    onOpenOperations = noopGameSelection,
  }: Props = $props();

  const hasGames = $derived(games.length > 0);
  const showEmptyState = $derived(!hasGames && !busy);
  const showInitialBusyState = $derived(!hasGames && busy);

  const scanButtonLabel = $derived(busy ? t('games.scanning') : t('games.scanFolder'));
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
    model.hasFilterIndicator ? t('games.openFiltersActive') : t('games.openFilters'),
  );

  const favoritesButtonLabel = $derived(
    model.filtersState.appliedFavoritesOnly
      ? t('games.favoritesToggleActive')
      : t('games.favoritesToggle'),
  );

  const hiddenButtonLabel = $derived(
    model.filtersState.appliedShowHidden ? t('games.showHiddenActive') : t('games.showHidden'),
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

<section class="flex h-full min-h-0 flex-col gap-4" aria-busy={busy}>
  {#if showEmptyState}
    <div class="flex flex-1 flex-col items-center justify-center">
      <GamesEmptyState {busy} {scanButtonLabel} {onScan} />
    </div>
  {:else if showInitialBusyState}
    <Empty class="border-0" role="status" aria-live="polite" aria-atomic="true">
      <EmptyHeader>
        <EmptyMedia>
          <Spinner class="size-10" />
        </EmptyMedia>
        <EmptyTitle>{t('games.loading')}</EmptyTitle>
      </EmptyHeader>
    </Empty>
  {:else}
    <GamesHeaderBar {hasGames} {busy} {scanButtonLabel} {dashboardStats} {onScan} />

    <div class="grid shrink-0 gap-2 px-1">
      <div
        class="flex items-center justify-end gap-2 max-md:justify-stretch"
        role="search"
        aria-label={t('games.search')}
      >
        <label
          class="block max-w-88 min-w-48 shrink grow basis-88 max-md:max-w-none max-md:min-w-0"
        >
          <span class="sr-only">{t('games.search')}</span>

          <Input
            type="search"
            placeholder={t('games.search')}
            value={model.filtersState.searchQuery}
            oninput={handleSearchInput}
          />
        </label>

        <div class="flex flex-none items-center gap-1">
          <Button
            aria-label={favoritesButtonLabel}
            variant={model.filtersState.appliedFavoritesOnly ? 'default' : 'secondary'}
            size="icon-sm"
            onclick={model.quickToggleFavoritesOnly}
          >
            <StarIcon
              class={cn(
                'size-4.5',
                model.filtersState.appliedFavoritesOnly && 'fill-current text-yellow-300',
              )}
              aria-hidden="true"
            />
          </Button>

          <div class="relative inline-flex">
            <Button
              aria-label={hiddenButtonLabel}
              variant={model.filtersState.appliedShowHidden ? 'default' : 'secondary'}
              size="icon-sm"
              onclick={model.quickToggleShowHidden}
            >
              <EyeOffIcon class="size-4.5" aria-hidden="true" />
            </Button>
            {#if model.hiddenCount > 0}
              <span
                class="pointer-events-none absolute -top-1.5 -right-1.5 flex size-4 items-center justify-center rounded-full bg-primary text-[10px] font-medium text-primary-foreground ring-2 ring-background"
                aria-hidden="true"
              >
                {model.hiddenCount > 9 ? '9+' : model.hiddenCount}
              </span>
            {/if}
          </div>

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
    </div>

    <ScrollArea class="min-h-0 flex-1">
      <GamesGrid
        games={model.gameItems}
        launcherOrder={model.appliedLauncherOrder}
        {busy}
        {hasManualCoverAction}
        pickDisabled={pickCoverDisabled}
        {coversAutoFetchingIds}
        menuOpenFor={model.menuOpenFor}
        actionMenuRefs={model.actionMenuRefs}
        isCoverOperationBusy={model.isCoverOperationBusy}
        onMenuOpenChange={model.setMenuOpen}
        onFetchCover={model.fetchCover}
        onPickCover={model.pickCover}
        onClearCover={model.clearCover}
        onToggleFavorite={model.toggleFavorite}
        onToggleHidden={model.toggleHidden}
        onResetFilters={model.resetFilters}
        {onOpenDetails}
        {onOpenOperations}
      />
    </ScrollArea>
  {/if}
</section>
