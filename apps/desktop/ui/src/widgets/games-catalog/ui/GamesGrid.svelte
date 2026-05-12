<script lang="ts">
  import { type GameCardViewModel, type GameCardCoverMenuHandle, GameCard } from '@entities/game';

  type GameId = GameCardViewModel['id'];

  type GameActionHandler = (gameId: GameId) => void;
  type CoverBusyPredicate = (gameId: GameId) => boolean;
  type MenuOpenChangeHandler = (gameId: GameId, next: boolean) => void;

  type CoverMenuRefs = Readonly<Partial<Record<GameId, GameCardCoverMenuHandle>>>;

  type Props = {
    games?: readonly GameCardViewModel[];
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

  const EMPTY_GAMES: readonly GameCardViewModel[] = [];
  const EMPTY_AUTO_FETCHING_IDS: ReadonlySet<GameId> = new Set<GameId>();
  const EMPTY_COVER_MENU_REFS: CoverMenuRefs = {};

  const isCoverOperationIdle: CoverBusyPredicate = () => false;

  const {
    games = EMPTY_GAMES,
    busy = false,
    hasManualCoverAction = false,
    pickDisabled = false,
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
  <div
    class="grid grid-cols-[repeat(auto-fit,minmax(20.5rem,1fr))] items-stretch gap-3"
    aria-busy={busy}
  >
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
        {pickDisabled}
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
  <div class="px-1" aria-live="polite">
    <p class="leading-snug text-text-muted">No games match current filters.</p>
  </div>
{/if}
