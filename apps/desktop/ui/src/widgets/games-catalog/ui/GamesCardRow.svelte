<script lang="ts">
  import { GameCard, type GameCardFocusTarget } from '@entities/game';
  import type { GameCardState, GameId } from '../model/launcher-groups';

  type GameActionHandler = (gameId: GameId) => void;
  type Props = {
    cards: readonly GameCardState[];
    columnCount: number;
    onMenuOpenChange: (gameId: GameId, next: boolean) => void;
    onFetchCover: GameActionHandler;
    onPickCover: GameActionHandler;
    onClearCover: GameActionHandler;
    onToggleFavorite: (gameId: GameId, isFavorite: boolean) => void;
    onToggleHidden: (gameId: GameId, isHidden: boolean) => void;
    onOpenDetails: GameActionHandler;
    onPreloadDetails: () => void;
    onCardFocus: (gameId: GameId, target: GameCardFocusTarget) => void;
  };

  const {
    cards,
    columnCount,
    onMenuOpenChange,
    onFetchCover,
    onPickCover,
    onClearCover,
    onToggleFavorite,
    onToggleHidden,
    onOpenDetails,
    onPreloadDetails,
    onCardFocus,
  }: Props = $props();
</script>

<div
  class="grid items-stretch gap-3"
  style:grid-template-columns={`repeat(${columnCount}, minmax(0, 1fr))`}
>
  {#each cards as card (card.id)}
    <GameCard
      game={card.game}
      coverBusy={card.isCoverBusy}
      backgroundCoverFetching={card.isBackgroundCoverFetching}
      menuDisabled={card.isMenuDisabled}
      pickDisabled={card.isPickDisabled}
      menuOpen={card.isMenuOpen}
      coverMenuRef={card.menuRef}
      onMenuOpenChange={(next: boolean) => {
        onMenuOpenChange(card.id, next);
      }}
      onFetchCover={() => {
        onFetchCover(card.id);
      }}
      onPickCover={() => {
        onPickCover(card.id);
      }}
      onClearCover={() => {
        onClearCover(card.id);
      }}
      onToggleFavorite={() => {
        onToggleFavorite(card.id, !card.game.isFavorite);
      }}
      onToggleHidden={() => {
        onToggleHidden(card.id, !card.game.isHidden);
      }}
      onOpenDetails={() => {
        onOpenDetails(card.id);
      }}
      onFocusWithin={(event: FocusEvent) => {
        const element = event.target instanceof Element ? event.target : null;
        const target = element?.closest<HTMLElement>('[data-game-focus-target]')?.dataset
          .gameFocusTarget;
        onCardFocus(card.id, target === 'menu' ? 'menu' : 'details');
      }}
      {onPreloadDetails}
    />
  {/each}
</div>
