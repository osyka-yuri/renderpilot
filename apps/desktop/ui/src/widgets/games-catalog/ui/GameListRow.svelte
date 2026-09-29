<script lang="ts">
  import { t } from '@shared/i18n';
  import { Badge, Button, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import { createPresentedLibraries } from '@shared/graphics';
  import {
    addonCapabilityLabel,
    createTitleId,
    GameCardActionsMenu,
    GameCardCover,
    type GameCardFocusTarget,
  } from '@entities/game';
  import type { AddonCapability } from '@entities/game';
  import StarIcon from '@lucide/svelte/icons/star';
  import EyeOffIcon from '@lucide/svelte/icons/eye-off';
  import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';
  import type { GameCardState, GameId } from '../model/launcher-groups';

  type GameActionHandler = (gameId: GameId) => void;
  type Props = {
    card: GameCardState;
    onMenuOpenChange: (gameId: GameId, next: boolean) => void;
    onFetchCover: GameActionHandler;
    onPickCover: GameActionHandler;
    onClearCover: GameActionHandler;
    onToggleFavorite: (gameId: GameId, isFavorite: boolean) => void;
    onToggleHidden: (gameId: GameId, isHidden: boolean) => void;
    onRemoveGame: (gameId: GameId) => boolean | Promise<boolean>;
    onOpenDetails: GameActionHandler;
    onPreloadDetails: () => void;
    onCardFocus: (gameId: GameId, target: GameCardFocusTarget) => void;
  };

  let {
    card,
    onMenuOpenChange,
    onFetchCover,
    onPickCover,
    onClearCover,
    onToggleFavorite,
    onToggleHidden,
    onRemoveGame,
    onOpenDetails,
    onPreloadDetails,
    onCardFocus,
  }: Props = $props();

  const game = $derived(card.game);
  const titleId = $derived(createTitleId(game.id));
  const detailsLabel = $derived(t('game.card.action.detailsLabel', { title: game.title }));
  const presentedLibraries = $derived(createPresentedLibraries(game.libraries));
  const presentedAddons = $derived(
    game.addons.map((addon: AddonCapability) => addonCapabilityLabel(addon)),
  );
  const librarySummary = $derived(
    compactSummary(presentedLibraries.map((library) => library.label)),
  );
  const addonSummary = $derived(compactSummary(presentedAddons));

  function updateBadgeLabel(): string {
    if (game.updateBadge.kind === 'up-to-date') {
      return t('game.card.badge.upToDate');
    }
    if (game.updateBadge.count <= 0) {
      return t('game.card.badge.updatesAvailable');
    }
    return t('game.card.badge.updatesAvailableCount', { count: game.updateBadge.count });
  }

  function compactSummary(values: readonly string[]): {
    preview: string;
    complete: string;
    additionalCount: number;
  } | null {
    if (values.length === 0) {
      return null;
    }
    return {
      preview: values[0],
      complete: values.join(', '),
      additionalCount: values.length - 1,
    };
  }

  function handleFocusWithin(event: FocusEvent): void {
    const element = event.target instanceof Element ? event.target : null;
    const target = element?.closest<HTMLElement>('[data-game-focus-target]')?.dataset
      .gameFocusTarget;
    onCardFocus(game.id, target === 'menu' ? 'menu' : 'details');
  }
</script>

<article
  data-game-id={game.id}
  data-game-layout="list"
  aria-labelledby={titleId}
  onfocusin={handleFocusWithin}
  class="grid min-w-0 grid-cols-[2.75rem_minmax(0,1fr)] items-start gap-x-3 gap-y-2 rounded-lg border bg-card p-3 @lg:grid-cols-[2.75rem_minmax(0,1fr)_auto] @lg:items-center"
>
  <div class="min-w-0">
    <GameCardCover
      title={game.title}
      coverSrc={game.coverSrc}
      monogram={game.monogram}
      coverBusy={card.isCoverBusy}
    />
  </div>

  <div class="grid min-w-0 gap-1">
    <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
      <h3 id={titleId} class="line-clamp-2 min-w-0 text-sm/snug font-semibold wrap-break-word">
        {game.title}
        {#if game.isFavorite}
          <StarIcon
            class="ms-1 inline size-3.5 fill-yellow-500 text-yellow-500"
            aria-hidden="true"
          />
          <span class="sr-only">{t('game.card.status.favorite')}</span>
        {/if}
        {#if game.isHidden}
          <EyeOffIcon class="ms-1 inline size-3.5 text-muted-foreground" aria-hidden="true" />
          <span class="sr-only">{t('game.card.status.hidden')}</span>
        {/if}
      </h3>
      <Badge class="max-w-full" variant={game.updateBadge.variant}>
        {updateBadgeLabel()}
      </Badge>
    </div>

    {#if librarySummary}
      <div class="flex min-w-0 gap-1.5 text-xs/snug @lg:hidden">
        <span class="max-w-[48%] shrink-0 truncate font-medium text-muted-foreground">
          {t('game.card.detectedLibraries')}
        </span>
        <span class="min-w-0 truncate">
          <span aria-hidden="true">{librarySummary.preview}</span>
          {#if librarySummary.additionalCount > 0}
            <span aria-hidden="true" class="ms-1 text-muted-foreground">
              +{librarySummary.additionalCount}
            </span>
          {/if}
          <span class="sr-only">{librarySummary.complete}</span>
        </span>
      </div>
      <div class="hidden min-w-0 flex-wrap items-baseline gap-x-1.5 gap-y-1 text-xs/snug @lg:flex">
        <span class="shrink-0 font-medium text-muted-foreground">
          {t('game.card.detectedLibraries')}
        </span>
        <div class="flex min-w-0 flex-wrap gap-1">
          {#each presentedLibraries as library (library.tag)}
            <Badge class="max-w-full wrap-break-word whitespace-normal" variant="outline">
              {library.label}
            </Badge>
          {/each}
        </div>
      </div>
    {/if}

    {#if addonSummary}
      <div class="flex min-w-0 gap-1.5 text-xs/snug @lg:hidden">
        <span class="max-w-[48%] shrink-0 truncate font-medium text-muted-foreground">
          {t('game.card.availableAddons')}
        </span>
        <span class="min-w-0 truncate">
          <span aria-hidden="true">{addonSummary.preview}</span>
          {#if addonSummary.additionalCount > 0}
            <span aria-hidden="true" class="ms-1 text-muted-foreground">
              +{addonSummary.additionalCount}
            </span>
          {/if}
          <span class="sr-only">{addonSummary.complete}</span>
        </span>
      </div>
      <div class="hidden min-w-0 flex-wrap items-baseline gap-x-1.5 gap-y-1 text-xs/snug @lg:flex">
        <span class="shrink-0 font-medium text-muted-foreground">
          {t('game.card.availableAddons')}
        </span>
        <div class="flex min-w-0 flex-wrap gap-1">
          {#each presentedAddons as addon (addon)}
            <Badge class="max-w-full wrap-break-word whitespace-normal" variant="outline">
              {addon}
            </Badge>
          {/each}
        </div>
      </div>
    {/if}

    <p class="min-w-0 truncate text-xs text-muted-foreground" dir="auto">
      {game.installPath}
    </p>
  </div>

  <div class="col-start-2 flex items-center gap-2 justify-self-end @lg:col-start-3 @lg:row-start-1">
    <Tooltip ignoreNonKeyboardFocus>
      <TooltipTrigger>
        {#snippet child({ props })}
          <Button
            {...props}
            data-game-focus-target="details"
            data-game-details-trigger
            variant="default"
            size="icon-sm"
            aria-label={detailsLabel}
            onclick={() => {
              onOpenDetails(game.id);
            }}
            onpointerover={onPreloadDetails}
            onfocusin={onPreloadDetails}
          >
            <ArrowRightIcon class="rtl:rotate-180" aria-hidden="true" />
          </Button>
        {/snippet}
      </TooltipTrigger>
      <TooltipContent>{t('game.card.action.details')}</TooltipContent>
    </Tooltip>

    <GameCardActionsMenu
      title={game.title}
      disabled={card.isMenuDisabled}
      pickDisabled={card.isPickDisabled}
      autoFetchInProgress={card.isBackgroundCoverFetching}
      hasCover={game.hasCover}
      isFavorite={game.isFavorite}
      isHidden={game.isHidden}
      open={card.isMenuOpen}
      onOpenChange={(next: boolean) => {
        onMenuOpenChange(game.id, next);
      }}
      onFetchCover={() => {
        onFetchCover(game.id);
      }}
      onPickCover={() => {
        onPickCover(game.id);
      }}
      onClearCover={() => {
        onClearCover(game.id);
      }}
      onToggleFavorite={() => {
        onToggleFavorite(game.id, !game.isFavorite);
      }}
      onToggleHidden={() => {
        onToggleHidden(game.id, !game.isHidden);
      }}
      canRemoveFromCatalog={game.canRemoveFromCatalog}
      onRemoveFromCatalog={() => onRemoveGame(game.id)}
    />
  </div>
</article>
