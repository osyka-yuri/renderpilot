<script lang="ts">
  import { type GameCardViewModel, type Launcher, GameCard } from '@entities/game';
  import GamesFilterEmptyState from './GamesFilterEmptyState.svelte';
  import {
    createLauncherGroups,
    type ActionMenuRefs,
    type CardStateContext,
    type CoverBusyPredicate,
    type GameId,
    type LauncherGroup,
  } from '../model/launcher-groups';

  type GameActionHandler = (gameId: GameId) => void;
  type MenuOpenChangeHandler = (gameId: GameId, next: boolean) => void;

  type Props = {
    games?: readonly GameCardViewModel[];
    launcherOrder?: readonly Launcher[];
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
    onResetFilters?: () => void;
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
    onResetFilters = () => undefined,
  }: Props = $props();

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

  const launcherGroups = $derived.by<LauncherGroup[]>(() =>
    createLauncherGroups(games, launcherOrder, cardStateContext),
  );
</script>

<div class="flex flex-1 flex-col">
  {#if hasGames}
    <div class="flex flex-col gap-6" aria-busy={busy}>
      {#each launcherGroups as group (group.launcher)}
        <section class="flex flex-col gap-3">
          <h2 class="text-lg font-semibold text-foreground">{group.label}</h2>

          <div class="grid grid-cols-[repeat(auto-fit,minmax(20.5rem,1fr))] items-stretch gap-3">
            {#each group.cards as card (card.id)}
              <GameCard
                game={card.game}
                coverBusy={card.isCoverBusy}
                backgroundCoverFetching={card.isBackgroundCoverFetching}
                menuDisabled={card.isMenuDisabled}
                pickDisabled={card.isPickDisabled}
                menuOpen={card.isMenuOpen}
                coverMenuRef={card.menuRef}
                onMenuOpenChange={(next: boolean): void => {
                  onMenuOpenChange(card.id, next);
                }}
                onFetchCover={(): void => {
                  onFetchCover(card.id);
                }}
                onPickCover={(): void => {
                  onPickCover(card.id);
                }}
                onClearCover={(): void => {
                  onClearCover(card.id);
                }}
                onToggleFavorite={(): void => {
                  onToggleFavorite(card.id, !card.game.isFavorite);
                }}
                onToggleHidden={(): void => {
                  onToggleHidden(card.id, !card.game.isHidden);
                }}
                onOpenDetails={(): void => {
                  onOpenDetails(card.id);
                }}
              />
            {/each}
          </div>
        </section>
      {/each}
    </div>
  {:else}
    <GamesFilterEmptyState {onResetFilters} />
  {/if}
</div>
