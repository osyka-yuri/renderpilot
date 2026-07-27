<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import {
    type GameCardViewModel,
    type GameCardFocusTarget,
    type GamesCatalogScrollAnchor,
    type Launcher,
  } from '@entities/game';
  import GamesFilterEmptyState from './GamesFilterEmptyState.svelte';
  import GamesCardRow from './GamesCardRow.svelte';
  import {
    buildGamesVirtualRows,
    findGameVirtualRowIndex,
    findVisibleGamesAnchor,
    gamesGridColumnCount,
    pairExistingVirtualRows,
    shouldLoadMoreRows,
    type GamesVirtualRow,
  } from '../model/virtual-rows';
  import {
    createLauncherGroups,
    type ActionMenuRefs,
    type CardStateContext,
    type CoverBusyPredicate,
    type GameId,
  } from '../model/launcher-groups';

  type GameActionHandler = (gameId: GameId) => void;
  type MenuOpenChangeHandler = (gameId: GameId, next: boolean) => void;
  type Props = {
    games?: readonly GameCardViewModel[];
    launcherOrder?: readonly Launcher[];
    scrollElement?: HTMLElement | null;
    busy?: boolean;
    hasManualCoverAction?: boolean;
    pickDisabled?: boolean;
    coversAutoFetchingIds?: ReadonlySet<GameId>;
    menuOpenFor?: GameId | null;
    actionMenuRefs?: ActionMenuRefs;
    isCoverOperationBusy?: CoverBusyPredicate;
    onMenuOpenChange?: MenuOpenChangeHandler;
    onFetchCover?: GameActionHandler;
    onPickCover?: GameActionHandler;
    onClearCover?: GameActionHandler;
    onToggleFavorite?: (gameId: GameId, isFavorite: boolean) => void;
    onToggleHidden?: (gameId: GameId, isHidden: boolean) => void;
    onOpenDetails?: GameActionHandler;
    onLoadMore?: () => void;
    onResetFilters?: () => void;
    scrollAnchor?: GamesCatalogScrollAnchor | null;
    focusedGameId?: GameId | null;
    focusedTarget?: GameCardFocusTarget;
    onScrollAnchorChange?: (anchor: GamesCatalogScrollAnchor) => void;
    onCardFocus?: (gameId: GameId, target: GameCardFocusTarget) => void;
  };

  const EMPTY_GAMES: readonly GameCardViewModel[] = [];
  const EMPTY_LAUNCHER_ORDER: readonly Launcher[] = [];
  const EMPTY_AUTO_FETCHING_IDS: ReadonlySet<GameId> = new Set<GameId>();
  const EMPTY_ACTION_MENU_REFS: ActionMenuRefs = {};
  const noopAction: GameActionHandler = () => undefined;
  const noopMenuOpenChange: MenuOpenChangeHandler = () => undefined;
  const noopToggleFavorite = (_gameId: GameId, _isFavorite: boolean): void => undefined;
  const noopToggleHidden = (_gameId: GameId, _isHidden: boolean): void => undefined;
  const isCoverOperationIdle: CoverBusyPredicate = () => false;

  const {
    games = EMPTY_GAMES,
    launcherOrder = EMPTY_LAUNCHER_ORDER,
    scrollElement = null,
    busy = false,
    hasManualCoverAction = false,
    pickDisabled = false,
    coversAutoFetchingIds = EMPTY_AUTO_FETCHING_IDS,
    menuOpenFor = null,
    actionMenuRefs = EMPTY_ACTION_MENU_REFS,
    isCoverOperationBusy = isCoverOperationIdle,
    onMenuOpenChange = noopMenuOpenChange,
    onFetchCover = noopAction,
    onPickCover = noopAction,
    onClearCover = noopAction,
    onToggleFavorite = noopToggleFavorite,
    onToggleHidden = noopToggleHidden,
    onOpenDetails = noopAction,
    onLoadMore = () => undefined,
    onResetFilters = () => undefined,
    scrollAnchor = null,
    focusedGameId = null,
    focusedTarget = 'details',
    onScrollAnchorChange = () => undefined,
    onCardFocus = () => undefined,
  }: Props = $props();

  let rootElement = $state<HTMLElement | null>(null);
  let columnCount = $state(1);
  let layoutInitialized = $state(false);
  let initialAnchorRestored = $state(false);
  let restoringAnchor = $state(false);
  let restoredFocusKey: string | null = null;
  let lastPublishedAnchorKey = '';
  const hasGames = $derived(games.length > 0);
  const cardStateContext = $derived<CardStateContext>({
    busy,
    hasManualCoverAction,
    pickDisabled,
    coversAutoFetchingIds,
    menuOpenFor,
    actionMenuRefs,
    isCoverOperationBusy,
  });
  const launcherGroups = $derived(createLauncherGroups(games, launcherOrder, cardStateContext));
  const rows = $derived<GamesVirtualRow[]>(buildGamesVirtualRows(launcherGroups, columnCount));
  const rowVirtualizer = createVirtualizer<HTMLElement, HTMLElement>({
    count: 0,
    getScrollElement: () => scrollElement,
    estimateSize: () => 280,
    overscan: 4,
  });
  const virtualRows = $derived($rowVirtualizer.getVirtualItems());
  const renderedRows = $derived(pairExistingVirtualRows(virtualRows, rows));

  $effect(() => {
    const currentRows = rows;
    const viewport = scrollElement;
    untrack(() => $rowVirtualizer).setOptions({
      count: currentRows.length,
      getScrollElement: () => viewport,
      estimateSize: (index) => (currentRows[index]?.kind === 'header' ? 44 : 280),
      getItemKey: (index) => currentRows[index]?.key ?? index,
      overscan: 4,
    });
  });

  $effect(() => {
    const element = rootElement;
    if (!element) {
      return;
    }
    const updateColumns = async (width: number) => {
      const nextColumnCount = gamesGridColumnCount(width);
      if (!layoutInitialized) {
        columnCount = nextColumnCount;
        layoutInitialized = true;
        return;
      }
      if (nextColumnCount === columnCount) {
        return;
      }

      const anchor = captureVisibleAnchor();
      columnCount = nextColumnCount;
      await tick();
      $rowVirtualizer.measure();
      if (anchor) {
        await restoreAnchor(anchor);
      }
    };
    void updateColumns(element.clientWidth);
    const observer = new ResizeObserver((entries) => {
      void updateColumns(entries[0].contentRect.width);
    });
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  });

  $effect(() => {
    const lastVisible = renderedRows[renderedRows.length - 1]?.virtualRow.index ?? -1;
    if (shouldLoadMoreRows(rows.length, lastVisible)) {
      onLoadMore();
    }
  });

  $effect(() => {
    const anchor = scrollAnchor;
    if (!layoutInitialized || initialAnchorRestored || !scrollElement || rows.length === 0) {
      return;
    }
    initialAnchorRestored = true;
    if (anchor) {
      void restoreAnchor(anchor);
    }
  });

  $effect(() => {
    const visibleRowCount = renderedRows.length;
    if (!layoutInitialized || restoringAnchor || !scrollElement || visibleRowCount === 0) {
      return;
    }
    const anchor = captureVisibleAnchor();
    if (!anchor) {
      return;
    }
    const key = `${anchor.gameId}:${Math.round(anchor.offsetWithinRow)}`;
    if (key !== lastPublishedAnchorKey) {
      lastPublishedAnchorKey = key;
      onScrollAnchorChange(anchor);
    }
  });

  $effect(() => {
    const visibleRowCount = renderedRows.length;
    const gameId = focusedGameId;
    const focusKey = gameId ? `${gameId}:${focusedTarget}` : null;
    const root = rootElement;
    if (!gameId || !root || visibleRowCount === 0) {
      return;
    }
    const gameIsRendered = Array.from(root.querySelectorAll<HTMLElement>('[data-game-id]')).some(
      (element) => element.dataset.gameId === gameId,
    );
    if (!gameIsRendered) {
      restoredFocusKey = null;
      return;
    }
    if (restoredFocusKey === focusKey) {
      return;
    }
    void tick().then(() => {
      const card = Array.from(root.querySelectorAll<HTMLElement>('[data-game-id]')).find(
        (element) => element.dataset.gameId === gameId,
      );
      const trigger = card?.querySelector<HTMLElement>(
        `[data-game-focus-target="${focusedTarget}"]`,
      );
      if (trigger) {
        trigger.focus({ preventScroll: true });
        restoredFocusKey = focusKey;
      }
    });
  });

  function captureVisibleAnchor(): GamesCatalogScrollAnchor | null {
    const viewport = scrollElement;
    if (!viewport) {
      return null;
    }
    const scrollTop = viewport.scrollTop;
    return findVisibleGamesAnchor(rows, $rowVirtualizer.getVirtualItems(), scrollTop);
  }

  async function restoreAnchor(anchor: GamesCatalogScrollAnchor): Promise<void> {
    const viewport = scrollElement;
    const rowIndex = findGameVirtualRowIndex(rows, anchor.gameId);
    if (!viewport || rowIndex < 0) {
      return;
    }

    restoringAnchor = true;
    $rowVirtualizer.scrollToIndex(rowIndex, { align: 'start' });
    await tick();
    await new Promise<void>((resolve) => {
      requestAnimationFrame(() => {
        resolve();
      });
    });
    const measurement = $rowVirtualizer.getVirtualItems().find((item) => item.index === rowIndex);
    viewport.scrollTop = Math.max(
      0,
      (measurement?.start ?? viewport.scrollTop) + anchor.offsetWithinRow,
    );
    restoringAnchor = false;
  }
</script>

<div bind:this={rootElement} class="min-h-full w-full" aria-busy={busy}>
  {#if hasGames}
    <div class="relative w-full" style:height={`${$rowVirtualizer.getTotalSize()}px`}>
      {#each renderedRows as { virtualRow, row } (row.key)}
        <div
          use:$rowVirtualizer.measureElement
          data-index={virtualRow.index}
          class="absolute top-0 left-0 w-full pb-3"
          style:transform={`translateY(${virtualRow.start}px)`}
        >
          {#if row.kind === 'header'}
            <h2 class="pt-1 text-lg font-semibold text-foreground">{row.label}</h2>
          {:else}
            <GamesCardRow
              cards={row.cards}
              {columnCount}
              {onMenuOpenChange}
              {onFetchCover}
              {onPickCover}
              {onClearCover}
              {onToggleFavorite}
              {onToggleHidden}
              {onOpenDetails}
              {onCardFocus}
            />
          {/if}
        </div>
      {/each}
    </div>
  {:else}
    <GamesFilterEmptyState {onResetFilters} />
  {/if}
</div>
