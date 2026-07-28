<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';

  import {
    getDashboardStats,
    normalizeAddonCapabilities,
    type GameSelectionHandler,
    type GamesCatalogScrollAnchor,
    type GameCardFocusTarget,
  } from '@entities/game';
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
  import type { GamesCatalogSession } from '../model/create-games-page-model.svelte';

  type Props = {
    busy?: boolean;
    coversAutoFetchingIds?: ReadonlySet<string>;
    pickCoverDisabled?: boolean;

    onScan?: VoidHandler;
    onOpenDetails?: GameSelectionHandler;
    onPreloadDetails?: VoidHandler;
    session: GamesCatalogSession;
  };

  const noop: VoidHandler = () => undefined;
  const noopGameSelection: GameSelectionHandler = () => undefined;

  const {
    busy = false,
    coversAutoFetchingIds = new Set<string>(),
    pickCoverDisabled = false,

    onScan = noop,
    onOpenDetails = noopGameSelection,
    onPreloadDetails = noop,
    session: model,
  }: Props = $props();
  const hasGames = $derived(model.games.length > 0);
  const hasCatalog = $derived(model.catalogSize > 0);
  const waitingForBootstrap = $derived(model.bootstrapping || !model.filtersState.ready);
  const effectiveBusy = $derived(
    busy || waitingForBootstrap || (!hasCatalog && model.catalogSyncState === 'refreshing'),
  );
  const showEmptyState = $derived(!hasCatalog && !effectiveBusy);
  const showInitialBusyState = $derived(waitingForBootstrap || (!hasCatalog && effectiveBusy));
  const scanButtonLabel = $derived(effectiveBusy ? t('games.scanning') : t('games.scanFolder'));
  const dashboardStats = $derived(getDashboardStats(model.games));

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
  let scrollViewportRef = $state<HTMLElement | null>(null);

  onMount(() => {
    void tick().then(() => {
      const viewport = untrack(() => scrollViewportRef);
      if (viewport && !model.scrollAnchor) {
        viewport.scrollTop = model.scrollTop;
      }
    });

    return () => {
      if (scrollViewportRef) {
        model.setScrollTop(scrollViewportRef.scrollTop);
      }
    };
  });

  function handleSearchInput(event: Event & { currentTarget: HTMLInputElement }): void {
    model.setSearchQuery(event.currentTarget.value);
  }
</script>

<section class="flex h-full min-h-0 flex-col gap-4" aria-busy={effectiveBusy}>
  {#if showEmptyState}
    <div class="flex flex-1 flex-col items-center justify-center">
      <GamesEmptyState busy={effectiveBusy} {scanButtonLabel} {onScan} />
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
    <GamesHeaderBar {hasGames} busy={effectiveBusy} {scanButtonLabel} {dashboardStats} {onScan} />

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
            addonOptions={normalizeAddonCapabilities(model.filtersState.availableAddons)}
            draftAddons={model.filtersState.draftAddons}
            onDraftAddonsChange={model.handleDraftAddonsChange}
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

    <ScrollArea class="min-h-0 flex-1" bind:viewportRef={scrollViewportRef}>
      <GamesGrid
        scrollElement={scrollViewportRef}
        games={model.gameItems}
        launcherOrder={model.appliedLauncherOrder}
        busy={effectiveBusy}
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
        {onPreloadDetails}
        onLoadMore={model.loadNextPage}
        scrollAnchor={model.scrollAnchor}
        focusedGameId={model.focusedGameId}
        focusedTarget={model.focusedTarget}
        onScrollAnchorChange={(anchor: GamesCatalogScrollAnchor) => {
          model.setScrollAnchor(anchor);
        }}
        onCardFocus={(gameId: string, target: GameCardFocusTarget) => {
          model.setFocusedGame(gameId, target);
        }}
      />
    </ScrollArea>
  {/if}
</section>
