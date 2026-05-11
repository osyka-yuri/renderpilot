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

<div class="game-card">
  <Surface as="article" interactive shadow aria-labelledby={titleId}>
    <div class="card-content">
      <GameCardCoverMenu
        bind:this={coverMenuRef}
        title={game.title}
        disabled={menuDisabled}
        autoFetchInProgress={backgroundCoverFetch}
        hasCover={game.hasCover}
        open={menuOpen}
        onOpenChange={onMenuOpenChange}
        {onFetchCover}
        {onPickCover}
        {onClearCover}
      />

      <div class="card-header">
        <GameCardCoverPreview
          title={game.title}
          coverSrc={game.coverSrc}
          monogram={game.monogram}
          {coverBusy}
        />

        <div class="header-content">
          <BadgeGroup>
            <div class="game-card__update-badge">
              <Badge pill surface="soft" tone={game.updateBadge.tone} multiline>
                {game.updateBadge.label}
              </Badge>
            </div>
          </BadgeGroup>

          <div class="title-content">
            <h3 id={titleId} class="game-title">{game.title}</h3>
            <p class="card-path">{game.installPath}</p>
          </div>
        </div>
      </div>

      <div class="library-group">
        <p class="field-label">Detected libraries</p>
        <GameLibraryBadges libraries={game.libraries} />
      </div>

      <div class="card-actions">
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

<style>
  .game-card {
    position: relative;
    display: grid;
    min-width: 0;
    height: 100%;
    padding: var(--space-4);
    overflow: visible;
  }

  .card-content {
    position: relative;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    gap: var(--space-4);
    min-width: 0;
    height: 100%;
  }

  .card-header {
    display: grid;
    grid-template-columns: 4.75rem minmax(0, 1fr);
    align-items: start;
    gap: var(--space-3);
    min-width: 0;
  }

  .header-content,
  .title-content,
  .library-group {
    display: grid;
    min-width: 0;
  }

  .header-content {
    gap: var(--space-3);
    padding-inline-end: 2.75rem;
  }

  .title-content,
  .library-group {
    gap: var(--space-2);
  }

  .library-group {
    align-content: start;
  }

  .game-card__update-badge {
    max-width: min(100%, 15rem);
  }

  .game-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
    line-height: 1.2;
    overflow-wrap: anywhere;
  }

  .card-path {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.8125rem;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }

  .field-label {
    margin: 0 0 var(--space-1);
    color: var(--text-subtle);
    font-size: 0.6875rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .card-actions {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }

  @media (max-width: 720px) {
    .card-header {
      grid-template-columns: minmax(0, 1fr);
      gap: 0.9rem;
    }

    .card-actions {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  @media (max-width: 560px) {
    .game-card {
      padding: 0.9rem;
    }
  }
</style>
