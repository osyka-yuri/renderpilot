<script lang="ts">
  import { Badge, BadgeGroup, Button, Surface } from '@shared/ui';

  import GameCardCoverMenu from './GameCardCoverMenu.svelte';
  import GameLibraryBadges from './GameLibraryBadges.svelte';
  import type { GameCardViewModel } from '../model/game-card-view-model';
  import GameCardCoverPreview from './GameCardCoverPreview.svelte';
  import type { GameCardCoverMenuHandle } from './types';

  type VoidHandler = () => void;
  type MenuOpenChangeHandler = (open: boolean) => void;

  type Props = {
    game: GameCardViewModel;

    coverBusy?: boolean;
    backgroundCoverFetch?: boolean;
    menuDisabled?: boolean;
    pickDisabled?: boolean;
    menuOpen?: boolean;
    coverMenuRef?: GameCardCoverMenuHandle;

    onMenuOpenChange?: MenuOpenChangeHandler;
    onFetchCover?: VoidHandler;
    onPickCover?: VoidHandler;
    onClearCover?: VoidHandler;
    onOpenDetails?: VoidHandler;
    onOpenOperations?: VoidHandler;
  };

  const TITLE_ID_PREFIX = 'game-card-title';
  const UNKNOWN_ID_SEGMENT = 'unknown';

  const ACTION_LABELS = {
    details: 'Details',
    journal: 'Journal',
  } as const;

  const noop = (): void => {
    /* empty */
  };
  const noopMenuOpenChange: MenuOpenChangeHandler = () => {
    /* empty */
  };

  const normalizeDomIdSegment = (value: string): string => {
    const normalizedValue = value
      .trim()
      .replace(/\s+/g, '-')
      .replace(/[^a-zA-Z0-9_-]/g, '-')
      .replace(/-+/g, '-')
      .replace(/^-|-$/g, '')
      .toLowerCase();

    return normalizedValue || UNKNOWN_ID_SEGMENT;
  };

  const createTitleId = (gameId: string): string => {
    return `${TITLE_ID_PREFIX}-${normalizeDomIdSegment(gameId)}`;
  };

  const createActionAriaLabel = (action: string, gameTitle: string): string => {
    return `${action} for ${gameTitle}`;
  };

  let {
    game,

    coverBusy = false,
    backgroundCoverFetch = false,
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

  const detailsAriaLabel = $derived(createActionAriaLabel('Open details', game.title));

  const journalAriaLabel = $derived(createActionAriaLabel('Open journal', game.title));
</script>

<div class="relative grid h-full min-w-0 overflow-visible p-4">
  <Surface as="article" interactive shadow aria-labelledby={titleId}>
    <div
      class="
        relative grid h-full min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] gap-4
      "
    >
      <GameCardCoverMenu
        bind:this={coverMenuRef}
        title={game.title}
        disabled={menuDisabled}
        {pickDisabled}
        autoFetchInProgress={backgroundCoverFetch}
        hasCover={game.hasCover}
        open={menuOpen}
        onOpenChange={onMenuOpenChange}
        {onFetchCover}
        {onPickCover}
        {onClearCover}
      />

      <div
        class="
          grid min-w-0 grid-cols-[4.75rem_minmax(0,1fr)] items-start gap-3
          max-md:grid-cols-1 max-md:gap-3.5
        "
      >
        <GameCardCoverPreview
          title={game.title}
          coverSrc={game.coverSrc}
          monogram={game.monogram}
          {coverBusy}
        />

        <div class="grid min-w-0 gap-3 pe-11">
          <BadgeGroup>
            <div class="max-w-60">
              <Badge pill surface="soft" tone={game.updateBadge.tone} multiline>
                {game.updateBadge.label}
              </Badge>
            </div>
          </BadgeGroup>

          <div class="grid min-w-0 gap-2">
            <h3
              id={titleId}
              class="
              text-base/tight font-semibold wrap-break-word
            "
            >
              {game.title}
            </h3>
            <p class="text-xs/snug wrap-break-word text-text-muted">
              {game.installPath}
            </p>
          </div>
        </div>
      </div>

      <div class="grid min-w-0 content-start gap-2">
        <p class="mb-1 text-xs tracking-widest text-text-subtle uppercase">Detected libraries</p>
        <GameLibraryBadges libraries={game.libraries} />
      </div>

      <div
        class="
          grid grid-cols-2 gap-2
          max-md:grid-cols-1
        "
      >
        <Button
          variant="primary"
          size="sm"
          fullWidth
          aria-label={detailsAriaLabel}
          onclick={onOpenDetails}
        >
          {ACTION_LABELS.details}
        </Button>

        <Button
          variant="secondary"
          size="sm"
          fullWidth
          aria-label={journalAriaLabel}
          onclick={onOpenOperations}
        >
          {ACTION_LABELS.journal}
        </Button>
      </div>
    </div>
  </Surface>
</div>
