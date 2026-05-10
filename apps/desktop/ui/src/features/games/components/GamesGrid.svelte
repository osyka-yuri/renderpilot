<script lang="ts">
  import type { GameCardViewModel } from '@features/games/games-screen-model';
  import type { GameCardCoverMenuHandle } from './game-card-types';
  import GameCard from './GameCard.svelte';

  type GameId = GameCardViewModel['id'];

  type GameActionHandler = (gameId: GameId) => void;
  type CoverBusyPredicate = (gameId: GameId) => boolean;
  type MenuOpenChangeHandler = (gameId: GameId, next: boolean) => void;

  type CoverMenuRefs = Readonly<Partial<Record<GameId, GameCardCoverMenuHandle>>>;

  type Props = {
    games?: readonly GameCardViewModel[];
    busy?: boolean;
    hasManualCoverAction?: boolean;
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

  const EMPTY_GAMES: readonly GameCardViewModel[] = [];
  const EMPTY_AUTO_FETCHING_IDS: ReadonlySet<GameId> = new Set<GameId>();
  const EMPTY_COVER_MENU_REFS: CoverMenuRefs = {};

  const isCoverOperationIdle: CoverBusyPredicate = () => false;

  let {
    games = EMPTY_GAMES,
    busy = false,
    hasManualCoverAction = false,
    coversAutoFetchingIds = EMPTY_AUTO_FETCHING_IDS,
    menuOpenFor = null,
    coverMenuRefs = EMPTY_COVER_MENU_REFS,

    isCoverOperationBusy = isCoverOperationIdle,
    onMenuOpenChange,
    onFetchCover,
    onPickCover,
    onClearCover,
    onOpenDetails,
    onOpenOperations,
  }: Props = $props();

  function handleMenuOpenChange(gameId: GameId, next: boolean): void {
    onMenuOpenChange?.(gameId, next);
  }

  function handleGameAction(handler: GameActionHandler | undefined, gameId: GameId): void {
    handler?.(gameId);
  }
</script>

{#if games.length > 0}
  <div class="game-list" aria-busy={busy}>
    {#each games as game (game.id)}
      {@const gameId = game.id}
      {@const coverBusy = isCoverOperationBusy(gameId)}
      {@const backgroundCoverFetch = coversAutoFetchingIds.has(gameId)}
      {@const menuDisabled = busy || hasManualCoverAction || backgroundCoverFetch}
      {@const menuOpen = menuOpenFor === gameId}
      {@const coverMenuRef = coverMenuRefs[gameId]}

      <GameCard
        {game}
        {coverBusy}
        {backgroundCoverFetch}
        {menuDisabled}
        {menuOpen}
        {coverMenuRef}
        onMenuOpenChange={(next: boolean): void => {
          handleMenuOpenChange(gameId, next);
        }}
        onFetchCover={(): void => {
          handleGameAction(onFetchCover, gameId);
        }}
        onPickCover={(): void => {
          handleGameAction(onPickCover, gameId);
        }}
        onClearCover={(): void => {
          handleGameAction(onClearCover, gameId);
        }}
        onOpenDetails={(): void => {
          handleGameAction(onOpenDetails, gameId);
        }}
        onOpenOperations={(): void => {
          handleGameAction(onOpenOperations, gameId);
        }}
      />
    {/each}
  </div>
{:else}
  <div class="filtered-empty-state" aria-live="polite">
    <p class="filtered-empty-description">No games match current filters.</p>
  </div>
{/if}

<style>
  .game-list {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(20.5rem, 1fr));
    align-items: stretch;
    gap: var(--space-3);
  }

  .filtered-empty-state {
    display: block;
    padding: 0 var(--space-1);
  }

  .filtered-empty-description {
    margin: 0;
    color: var(--text-muted);
    line-height: 1.4;
  }
</style>
