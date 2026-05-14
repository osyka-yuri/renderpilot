<script lang="ts">
  import {
    type GameCardViewModel,
    type GameCardMenuHandle,
    type Launcher,
    GameCard,
    getLauncherDisplayLabel,
  } from '@entities/game';

  type GameId = GameCardViewModel['id'];

  type GameActionHandler = (gameId: GameId) => void;
  type CoverBusyPredicate = (gameId: GameId) => boolean;
  type MenuOpenChangeHandler = (gameId: GameId, next: boolean) => void;

  type CoverMenuRefs = Readonly<Partial<Record<GameId, GameCardMenuHandle>>>;

  type Props = {
    games?: readonly GameCardViewModel[];
    launcherOrder?: readonly Launcher[];
    busy?: boolean;
    hasManualCoverAction?: boolean;
    pickDisabled?: boolean;
    coversAutoFetchingIds?: ReadonlySet<GameId>;
    menuOpenFor?: GameId | null;
    coverMenuRefs?: CoverMenuRefs;

    isCoverOperationBusy?: CoverBusyPredicate;
    onMenuOpenChange?: MenuOpenChangeHandler;
    onFetchCover?: GameActionHandler;
    onPickCover?: GameActionHandler;
    onClearCover?: GameActionHandler;
    onOpenDetails?: GameActionHandler;
    onOpenOperations?: GameActionHandler;
  };

  type LauncherGameGroup = {
    launcher: Launcher;
    label: string;
    games: readonly GameCardViewModel[];
  };

  type CardStateContext = {
    busy: boolean;
    hasManualCoverAction: boolean;
    pickDisabled: boolean;
    coversAutoFetchingIds: ReadonlySet<GameId>;
    menuOpenFor: GameId | null;
    coverMenuRefs: CoverMenuRefs;
    isCoverOperationBusy: CoverBusyPredicate;
  };

  type GameCardState = {
    game: GameCardViewModel;
    id: GameId;
    isCoverBusy: boolean;
    isBackgroundCoverFetching: boolean;
    isMenuDisabled: boolean;
    isPickDisabled: boolean;
    isMenuOpen: boolean;
    menuRef?: GameCardMenuHandle;
  };

  type LauncherGroup = {
    launcher: Launcher;
    label: string;
    cards: readonly GameCardState[];
  };

  const EMPTY_GAMES: readonly GameCardViewModel[] = [];
  const EMPTY_LAUNCHER_ORDER: readonly Launcher[] = [];
  const EMPTY_AUTO_FETCHING_IDS: ReadonlySet<GameId> = new Set<GameId>();
  const EMPTY_COVER_MENU_REFS: CoverMenuRefs = {};

  const noopAction: GameActionHandler = () => undefined;
  const noopMenuOpenChange: MenuOpenChangeHandler = () => undefined;
  const isCoverOperationIdle: CoverBusyPredicate = () => false;

  let {
    games = EMPTY_GAMES,
    launcherOrder = EMPTY_LAUNCHER_ORDER,
    busy = false,
    hasManualCoverAction = false,
    pickDisabled = false,
    coversAutoFetchingIds = EMPTY_AUTO_FETCHING_IDS,
    menuOpenFor = null,
    coverMenuRefs = EMPTY_COVER_MENU_REFS,

    isCoverOperationBusy = isCoverOperationIdle,
    onMenuOpenChange = noopMenuOpenChange,
    onFetchCover = noopAction,
    onPickCover = noopAction,
    onClearCover = noopAction,
    onOpenDetails = noopAction,
    onOpenOperations = noopAction,
  }: Props = $props();

  const hasGames = $derived(games.length > 0);

  const cardStateContext = $derived<CardStateContext>({
    busy,
    hasManualCoverAction,
    pickDisabled,
    coversAutoFetchingIds,
    menuOpenFor,
    coverMenuRefs,
    isCoverOperationBusy,
  });

  const launcherGroups = $derived.by(() =>
    createLauncherGroups(games, launcherOrder, cardStateContext),
  );

  function createLauncherGroups(
    games: readonly GameCardViewModel[],
    launcherOrder: readonly Launcher[],
    cardContext: CardStateContext,
  ): LauncherGroup[] {
    return groupGamesByLauncher(games, launcherOrder).map((group) => ({
      launcher: group.launcher,
      label: group.label,
      cards: group.games.map((game) => createGameCardState(game, cardContext)),
    }));
  }

  function createGameCardState(game: GameCardViewModel, context: CardStateContext): GameCardState {
    const id = game.id;
    const isBackgroundCoverFetching = context.coversAutoFetchingIds.has(id);

    return {
      game,
      id,
      isCoverBusy: context.isCoverOperationBusy(id),
      isBackgroundCoverFetching,
      isMenuDisabled: context.busy || context.hasManualCoverAction || isBackgroundCoverFetching,
      isPickDisabled: context.pickDisabled,
      isMenuOpen: context.menuOpenFor === id,
      menuRef: context.coverMenuRefs[id],
    };
  }

  function groupGamesByLauncher(
    games: readonly GameCardViewModel[],
    launcherOrder: readonly Launcher[],
  ): LauncherGameGroup[] {
    if (games.length === 0) {
      return [];
    }

    const gamesByLauncher = createGamesByLauncherIndex(games);
    const launchers = getLaunchersInDisplayOrder(gamesByLauncher, launcherOrder);

    return launchers.map((launcher) =>
      createLauncherGameGroup(launcher, gamesByLauncher.get(launcher) ?? []),
    );
  }

  function createGamesByLauncherIndex(
    games: readonly GameCardViewModel[],
  ): Map<Launcher, GameCardViewModel[]> {
    // Local Map is rebuilt inside $derived, so Svelte reactivity is not needed here.
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const gamesByLauncher = new Map<Launcher, GameCardViewModel[]>();

    for (const game of games) {
      const groupGames = gamesByLauncher.get(game.launcher);

      if (groupGames) {
        groupGames.push(game);
      } else {
        gamesByLauncher.set(game.launcher, [game]);
      }
    }

    return gamesByLauncher;
  }

  function getLaunchersInDisplayOrder(
    gamesByLauncher: ReadonlyMap<Launcher, readonly GameCardViewModel[]>,
    launcherOrder: readonly Launcher[],
  ): Launcher[] {
    const orderedLaunchers = getExistingLaunchersFromOrder(gamesByLauncher, launcherOrder);
    const remainingLaunchers = getRemainingLaunchers(gamesByLauncher, orderedLaunchers);

    return [...orderedLaunchers, ...remainingLaunchers];
  }

  function getExistingLaunchersFromOrder(
    gamesByLauncher: ReadonlyMap<Launcher, readonly GameCardViewModel[]>,
    launcherOrder: readonly Launcher[],
  ): Launcher[] {
    const launchers: Launcher[] = [];

    for (const launcher of launcherOrder) {
      if (launchers.includes(launcher) || !gamesByLauncher.has(launcher)) {
        continue;
      }

      launchers.push(launcher);
    }

    return launchers;
  }

  function getRemainingLaunchers(
    gamesByLauncher: ReadonlyMap<Launcher, readonly GameCardViewModel[]>,
    orderedLaunchers: readonly Launcher[],
  ): Launcher[] {
    return Array.from(gamesByLauncher.keys())
      .filter((launcher) => !orderedLaunchers.includes(launcher))
      .sort(compareLaunchersByLabel);
  }

  function createLauncherGameGroup(
    launcher: Launcher,
    games: readonly GameCardViewModel[],
  ): LauncherGameGroup {
    return {
      launcher,
      label: getLauncherDisplayLabel(launcher),
      games,
    };
  }

  function compareLaunchersByLabel(left: Launcher, right: Launcher): number {
    return getLauncherDisplayLabel(left).localeCompare(getLauncherDisplayLabel(right), undefined, {
      sensitivity: 'base',
    });
  }
</script>

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
              onOpenDetails={(): void => {
                onOpenDetails(card.id);
              }}
              onOpenOperations={(): void => {
                onOpenOperations(card.id);
              }}
            />
          {/each}
        </div>
      </section>
    {/each}
  </div>
{:else}
  <div class="px-1" aria-live="polite">
    <p class="leading-snug text-muted-foreground">No games match current filters.</p>
  </div>
{/if}
