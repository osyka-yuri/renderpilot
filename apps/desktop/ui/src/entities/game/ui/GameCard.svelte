<script lang="ts">
  import {
    Badge,
    Button,
    Card,
    CardAction,
    CardContent,
    CardFooter,
    CardHeader,
    CardTitle,
  } from '@shared/ui';

  import { t } from '@shared/i18n';
  import { createTitleId } from '../model/dom-helpers';
  import type { GameCardViewModel, UpdateBadge } from '../model/game-card-view-model';

  import StarIcon from '@lucide/svelte/icons/star';
  import EyeOffIcon from '@lucide/svelte/icons/eye-off';
  import GameCardActionsMenu from './GameCardActionsMenu.svelte';
  import GameCardCover from './GameCardCover.svelte';
  import GameLibraryBadges from './GameLibraryBadges.svelte';
  import type { GameCardMenuHandle } from './types';

  type VoidHandler = () => void;
  type MenuOpenChangeHandler = (open: boolean) => void;

  type Props = {
    game: GameCardViewModel;

    coverBusy?: boolean;
    backgroundCoverFetching?: boolean;
    menuDisabled?: boolean;
    pickDisabled?: boolean;
    menuOpen?: boolean;
    coverMenuRef?: GameCardMenuHandle;

    onMenuOpenChange?: MenuOpenChangeHandler;
    onFetchCover?: VoidHandler;
    onPickCover?: VoidHandler;
    onClearCover?: VoidHandler;
    onToggleFavorite?: VoidHandler;
    onToggleHidden?: VoidHandler;
    onOpenDetails?: VoidHandler;
  };

  const noop: VoidHandler = () => undefined;
  const noopMenuOpenChange: MenuOpenChangeHandler = () => undefined;

  const HEADER_LAYOUT_CLASS =
    'grid min-w-0 grid-cols-[4.75rem_minmax(0,1fr)] items-start gap-3 max-md:grid-cols-1 max-md:gap-3.5';

  let {
    game,

    coverBusy = false,
    backgroundCoverFetching = false,
    menuDisabled = false,
    pickDisabled = false,
    menuOpen = false,
    coverMenuRef = $bindable(),

    onMenuOpenChange = noopMenuOpenChange,
    onFetchCover = noop,
    onPickCover = noop,
    onClearCover = noop,
    onToggleFavorite = noop,
    onToggleHidden = noop,
    onOpenDetails = noop,
  }: Props = $props();

  const titleId = $derived(createTitleId(game.id));

  const detailsAriaLabel = $derived(t('game.card.action.detailsAria', { title: game.title }));

  function updateBadgeLabel(badge: UpdateBadge): string {
    if (badge.kind === 'up-to-date') {
      return t('game.card.badge.upToDate');
    }

    if (badge.count <= 0) {
      return t('game.card.badge.updatesAvailable');
    }

    return t('game.card.badge.updatesAvailableCount', { count: badge.count });
  }
</script>

<Card aria-labelledby={titleId}>
  <CardHeader>
    <CardAction>
      <GameCardActionsMenu
        bind:this={coverMenuRef}
        title={game.title}
        disabled={menuDisabled}
        {pickDisabled}
        autoFetchInProgress={backgroundCoverFetching}
        hasCover={game.hasCover}
        isFavorite={game.isFavorite}
        isHidden={game.isHidden}
        open={menuOpen}
        onOpenChange={onMenuOpenChange}
        {onFetchCover}
        {onPickCover}
        {onClearCover}
        {onToggleFavorite}
        {onToggleHidden}
      />
    </CardAction>

    <div class={HEADER_LAYOUT_CLASS}>
      <GameCardCover
        title={game.title}
        coverSrc={game.coverSrc}
        monogram={game.monogram}
        {coverBusy}
      />

      <div class="grid min-w-0 gap-3">
        <Badge class="w-fit" variant={game.updateBadge.variant}>
          {updateBadgeLabel(game.updateBadge)}
        </Badge>

        <div class="grid min-w-0 gap-2">
          <CardTitle id={titleId} role="heading" aria-level={3} class="flex items-center gap-2">
            {game.title}
            {#if game.isFavorite}
              <StarIcon class="size-4 text-yellow-500 fill-yellow-500" aria-label="Favorite" />
            {/if}
            {#if game.isHidden}
              <EyeOffIcon class="size-4 text-muted-foreground" aria-label="Hidden" />
            {/if}
          </CardTitle>

          <p class="min-w-0 text-xs/snug break-all text-muted-foreground">
            {game.installPath}
          </p>
        </div>
      </div>
    </div>
  </CardHeader>

  <CardContent class="flex-1">
    <p class="mb-1 text-xs font-medium tracking-wider text-muted-foreground uppercase">
      {t('game.card.detectedLibraries')}
    </p>

    <GameLibraryBadges libraries={game.libraries} />
  </CardContent>

  <CardFooter>
    <Button
      class="w-full"
      variant="default"
      size="sm"
      aria-label={detailsAriaLabel}
      onclick={onOpenDetails}
    >
      {t('game.card.action.details')}
    </Button>
  </CardFooter>
</Card>
