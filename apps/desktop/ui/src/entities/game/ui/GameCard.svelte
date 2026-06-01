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
    onOpenDetails?: VoidHandler;
    onOpenOperations?: VoidHandler;
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
    onOpenDetails = noop,
    onOpenOperations = noop,
  }: Props = $props();

  const titleId = $derived(createTitleId(game.id));

  const actionAriaLabels = $derived({
    details: t('game.card.action.detailsAria', { title: game.title }),
    journal: t('game.card.action.journalAria', { title: game.title }),
  });

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
        open={menuOpen}
        onOpenChange={onMenuOpenChange}
        {onFetchCover}
        {onPickCover}
        {onClearCover}
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
          <CardTitle id={titleId} role="heading" aria-level={3}>
            {game.title}
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

  <CardFooter class="gap-2 *:flex-1">
    <Button
      variant="default"
      size="sm"
      aria-label={actionAriaLabels.details}
      onclick={onOpenDetails}
    >
      {t('game.card.action.details')}
    </Button>

    <Button
      variant="secondary"
      size="sm"
      aria-label={actionAriaLabels.journal}
      onclick={onOpenOperations}
    >
      {t('game.card.action.journal')}
    </Button>
  </CardFooter>
</Card>
