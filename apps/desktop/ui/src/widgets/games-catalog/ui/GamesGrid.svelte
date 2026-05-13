<script lang="ts">
  import { type GameCardViewModel, type GameCardMenuHandle, GameCard } from '@entities/game';

  type GameId = GameCardViewModel['id'];

  type GameActionHandler = (gameId: GameId) => void;
  type CoverBusyPredicate = (gameId: GameId) => boolean;
  type MenuOpenChangeHandler = (gameId: GameId, next: boolean) => void;

  type CoverMenuRefs = Readonly<Partial<Record<GameId, GameCardMenuHandle>>>;

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
    onMenuOpenChange = () => undefined,
    onFetchCover = () => undefined,
    onPickCover = () => undefined,
    onClearCover = () => undefined,
    onOpenDetails = () => undefined,
    onOpenOperations = () => undefined,
  }: Props = $props();
</script>

{#if games.length > 0}
  <div
    class="grid grid-cols-[repeat(auto-fit,minmax(20.5rem,1fr))] items-stretch gap-3"
    aria-busy={busy}
  >
    {#each games as game (game.id)}
      {@const gameId = game.id}
      {@const coverBusy = isCoverOperationBusy(gameId)}
      {@const backgroundCoverFetching = coversAutoFetchingIds.has(gameId)}
      {@const menuDisabled = busy || hasManualCoverAction || backgroundCoverFetching}
      {@const menuOpen = menuOpenFor === gameId}
      {@const coverMenuRef = coverMenuRefs[gameId]}

      <GameCard
        {game}
        {coverBusy}
        {backgroundCoverFetching}
        {menuDisabled}
        {pickDisabled}
        {menuOpen}
        {coverMenuRef}
        onMenuOpenChange={(next: boolean): void => {
          onMenuOpenChange(gameId, next);
        }}
        onFetchCover={(): void => {
          onFetchCover(gameId);
        }}
        onPickCover={(): void => {
          onPickCover(gameId);
        }}
        onClearCover={(): void => {
          onClearCover(gameId);
        }}
        onOpenDetails={(): void => {
          onOpenDetails(gameId);
        }}
        onOpenOperations={(): void => {
          onOpenOperations(gameId);
        }}
      />
    {/each}
  </div>
{:else}
  <div class="px-1" aria-live="polite">
    <p class="leading-snug text-muted-foreground">No games match current filters.</p>
  </div>
{/if}
