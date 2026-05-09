<script lang="ts">
  import type { GameCard } from '@shared/api/types';
  import type { GameSelectionHandler, VoidHandler } from '@shared/utils/callbacks';
  import {
    clearGameCover,
    fetchGameCover,
    isDesktopPreviewMode,
    setGameCover,
  } from '@shared/api/desktop';
  import { describeCommandError } from '@shared/api/errors';
  import Badge from '@shared/ui/Badge.svelte';
  import BadgeGroup from '@shared/ui/BadgeGroup.svelte';
  import Button from '@shared/ui/Button.svelte';
  import Surface from '@shared/ui/Surface.svelte';
  import GameTechnologyBadges from '@features/games/GameTechnologyBadges.svelte';
  import GameCardCoverMenu from '@features/games/GameCardCoverMenu.svelte';
  import GamesDashboardSummary from '@features/games/GamesDashboardSummary.svelte';
  import GamesEmptyState from '@features/games/GamesEmptyState.svelte';
  import { getDashboardStats, toGameCardViewModel } from '@features/games/games-screen-model';
  import { open } from '@tauri-apps/plugin-dialog';

  const SCAN_LABEL = 'Scan Folder';
  const SCANNING_LABEL = 'Scanning...';
  const DESKTOP_APP_REQUIRED_MESSAGE = 'Choosing a cover file requires the desktop app.';

  const COVER_IMAGE_FILTERS = [
    { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'] },
  ];

  const noop: VoidHandler = (): void => {
    // Intentionally empty.
  };

  const noopOpenGame: GameSelectionHandler = (): void => {
    // Intentionally empty.
  };

  const noopReloadCards = async (): Promise<void> => {
    // Intentionally empty.
  };

  const noopCoverError = (): void => {
    // Intentionally empty.
  };

  export let games: GameCard[] = [];
  export let busy = false;
  /** Game IDs currently receiving automatic cover fetch (DesktopApp background sync). */
  export let coversAutoFetchingIds: ReadonlySet<string> = new Set();

  export let onScan: VoidHandler = noop;
  export let onRefresh: VoidHandler = noop;
  /** Reload game cards only (e.g. after cover change); avoids full library rescan. */
  export let onReloadCards: () => Promise<void> = noopReloadCards;

  export let onClearError: VoidHandler = noop;
  export let onCoverError: (message: string) => void = noopCoverError;
  export let onOpenDetails: GameSelectionHandler = noopOpenGame;
  export let onOpenOperations: GameSelectionHandler = noopOpenGame;

  type GameCardCoverMenuHandle = {
    focusTrigger: () => void;
  };

  type CoverMenuRefs = Record<string, GameCardCoverMenuHandle | undefined>;

  let manualCoverBusyFor: string | null = null;
  let menuOpenFor: string | null = null;
  let coverMenuRefs: CoverMenuRefs = {};

  $: gameItems = games.map(toGameCardViewModel);
  $: gameIds = gameItems.map((game) => game.id);
  $: hasGames = gameItems.length > 0;
  $: scanButtonLabel = busy ? SCANNING_LABEL : SCAN_LABEL;
  $: dashboardStats = getDashboardStats(games);
  $: hasManualCoverAction = manualCoverBusyFor !== null;

  $: pruneCoverMenuRefs(gameIds);

  $: if (menuOpenFor !== null && (hasManualCoverAction || coversAutoFetchingIds.has(menuOpenFor))) {
    menuOpenFor = null;
  }

  function pruneCoverMenuRefs(activeGameIds: readonly string[]): void {
    const activeIds = new Set(activeGameIds);
    let didPrune = false;
    const nextRefs: CoverMenuRefs = {};

    for (const [gameId, menuRef] of Object.entries(coverMenuRefs)) {
      if (activeIds.has(gameId)) {
        nextRefs[gameId] = menuRef;
      } else {
        didPrune = true;
      }
    }

    if (didPrune) {
      coverMenuRefs = nextRefs;
    }

    if (menuOpenFor !== null && !activeIds.has(menuOpenFor)) {
      menuOpenFor = null;
    }
  }

  function isManualCoverBusy(gameId: string): boolean {
    return manualCoverBusyFor === gameId;
  }

  /** Manual card cover action or background auto-fetch (overlay / aria-busy). */
  function isCoverOperationBusy(gameId: string): boolean {
    return isManualCoverBusy(gameId) || coversAutoFetchingIds.has(gameId);
  }

  function setMenuOpen(gameId: string, open: boolean): void {
    menuOpenFor = open ? gameId : null;
  }

  function closeMenu(): void {
    menuOpenFor = null;
  }

  function focusMenuTrigger(gameId: string): void {
    const focus = (): void => {
      coverMenuRefs[gameId]?.focusTrigger();
    };

    if (typeof requestAnimationFrame === 'function') {
      requestAnimationFrame(focus);
      return;
    }

    focus();
  }

  async function withManualCoverBusy(gameId: string, task: () => Promise<void>): Promise<void> {
    if (manualCoverBusyFor !== null) {
      return;
    }

    manualCoverBusyFor = gameId;

    try {
      await task();
      onClearError();
      await onReloadCards();
    } catch (error: unknown) {
      onCoverError(describeCommandError(error));
    } finally {
      manualCoverBusyFor = null;
      focusMenuTrigger(gameId);
    }
  }

  async function runCoverCommand(gameId: string, command: () => Promise<void>): Promise<void> {
    closeMenu();
    await withManualCoverBusy(gameId, command);
  }

  async function selectCoverPath(): Promise<string | null> {
    const selected = await open({
      multiple: false,
      filters: COVER_IMAGE_FILTERS,
    });

    return selected;
  }

  async function handleFetchCover(gameId: string): Promise<void> {
    await runCoverCommand(gameId, async () => {
      await fetchGameCover(gameId);
    });
  }

  async function handleClearCover(gameId: string): Promise<void> {
    await runCoverCommand(gameId, async () => {
      await clearGameCover(gameId);
    });
  }

  async function handlePickCover(gameId: string): Promise<void> {
    closeMenu();

    if (manualCoverBusyFor !== null) {
      return;
    }

    if (isDesktopPreviewMode()) {
      onCoverError(DESKTOP_APP_REQUIRED_MESSAGE);
      focusMenuTrigger(gameId);
      return;
    }

    let selectedPath: string | null = null;

    try {
      selectedPath = await selectCoverPath();
    } catch (error: unknown) {
      onCoverError(describeCommandError(error));
      focusMenuTrigger(gameId);
      return;
    }

    if (selectedPath === null) {
      focusMenuTrigger(gameId);
      return;
    }

    await withManualCoverBusy(gameId, async () => {
      await setGameCover(gameId, selectedPath);
    });
  }
</script>

<section class="screen-shell" aria-busy={busy}>
  <div class="overview-bar">
    {#if hasGames}
      <GamesDashboardSummary stats={dashboardStats} />
    {/if}

    <div class="action-group">
      <Button variant="secondary" size="sm" disabled={busy} loading={busy} onclick={onRefresh}>
        Refresh Libraries
      </Button>

      <Button variant="primary" size="sm" disabled={busy} loading={busy} onclick={onScan}>
        {scanButtonLabel}
      </Button>
    </div>
  </div>

  {#if !hasGames}
    <GamesEmptyState {busy} {scanButtonLabel} {onRefresh} {onScan} />
  {:else}
    <div class="game-list">
      {#each gameItems as game (game.id)}
        {@const coverBusy = isCoverOperationBusy(game.id)}
        {@const backgroundCoverFetch = coversAutoFetchingIds.has(game.id)}
        {@const menuDisabled = busy || hasManualCoverAction || backgroundCoverFetch}

        <Surface as="article" interactive shadow class="game-card">
          <div class="card-body">
            <GameCardCoverMenu
              bind:this={coverMenuRefs[game.id]}
              title={game.title}
              disabled={menuDisabled}
              autoFetchInProgress={backgroundCoverFetch}
              hasCover={game.hasCover}
              open={menuOpenFor === game.id}
              onOpenChange={(next: boolean): void => {
                setMenuOpen(game.id, next);
              }}
              onFetchCover={(): void => {
                void handleFetchCover(game.id);
              }}
              onPickCover={(): void => {
                void handlePickCover(game.id);
              }}
              onClearCover={(): void => {
                void handleClearCover(game.id);
              }}
            />

            <div class="card-header">
              <div
                class="cover-stack"
                aria-busy={coverBusy ? 'true' : 'false'}
                aria-label={`Cover artwork: ${game.title}`}
              >
                {#if game.coverSrc}
                  <img
                    class="cover-image"
                    src={game.coverSrc}
                    alt=""
                    loading="lazy"
                    decoding="async"
                  />
                {:else}
                  <div class="cover-placeholder" aria-hidden="true">
                    <span>{game.monogram}</span>
                  </div>
                {/if}

                {#if coverBusy}
                  <div class="cover-busy-overlay" aria-hidden="true"></div>
                {/if}
              </div>

              <div class="header-copy">
                <BadgeGroup class="platform-row">
                  <Badge pill surface="soft" tone={game.updateBadge.tone} multiline>
                    {game.updateBadge.label}
                  </Badge>
                </BadgeGroup>

                <div class="title-copy">
                  <h3 class="game-title">{game.title}</h3>
                  <p class="card-path">{game.installPath}</p>
                </div>
              </div>
            </div>

            <div class="technology-group">
              <p class="field-label">Detected libraries</p>
              <GameTechnologyBadges technologies={game.technologies} />
            </div>

            <div class="card-actions">
              <Button
                variant="primary"
                size="sm"
                fullWidth
                aria-label={`Open details for ${game.title}`}
                onclick={(): void => {
                  onOpenDetails(game.id);
                }}
              >
                Details
              </Button>

              <Button
                variant="secondary"
                size="sm"
                fullWidth
                aria-label={`Open journal for ${game.title}`}
                onclick={(): void => {
                  onOpenOperations(game.id);
                }}
              >
                Journal
              </Button>
            </div>
          </div>
        </Surface>
      {/each}
    </div>
  {/if}
</section>

<style>
  .screen-shell {
    display: grid;
    gap: var(--space-4);
  }

  .overview-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3) var(--space-4);
    padding: 0 var(--space-1);
  }

  .action-group {
    display: flex;
    flex-wrap: wrap;
  }

  :global(.dashboard-summary) {
    gap: var(--space-2);
  }

  .action-group {
    gap: var(--space-2);
  }

  .overview-bar .action-group {
    margin-left: auto;
  }

  .game-list {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(20.5rem, 1fr));
    align-items: stretch;
    gap: var(--space-3);
  }

  :global(.game-card) {
    position: relative;
    display: grid;
    min-width: 0;
    height: 100%;
    padding: var(--space-4);
    overflow: visible;
  }

  .card-body {
    position: relative;

    display: grid;
    grid-template-rows: auto 1fr auto;
    gap: var(--space-4);
    min-width: 0;
    height: 100%;
  }

  .card-header {
    display: grid;
    grid-template-columns: 4.75rem minmax(0, 1fr);
    align-items: start;
    gap: var(--space-3);
  }

  .cover-stack {
    position: relative;
    width: 100%;
    aspect-ratio: 600 / 900;
    overflow: hidden;
    border-radius: var(--radius-lg);
    border: 1px solid color-mix(in srgb, var(--accent-outline) 48%, var(--border-subtle));
    box-shadow: inset 0 1px 0 color-mix(in srgb, white 10%, transparent);
    background: var(--bg-control);
  }

  .cover-image,
  .cover-placeholder {
    border: none;
    border-radius: 0;
    box-shadow: none;
    min-height: 0;
  }

  .cover-image {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center top;
    background: var(--bg-control);
    pointer-events: none;
    user-select: none;
  }

  .cover-placeholder {
    display: grid;
    height: 100%;
    align-content: center;
    justify-items: center;
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--accent) 16%, var(--bg-control)) 0%,
      var(--bg-control) 100%
    );
    color: var(--text-strong);
    pointer-events: none;
    user-select: none;
  }

  .cover-placeholder span {
    font-size: 1.45rem;
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .cover-busy-overlay {
    position: absolute;
    inset: 0;
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--bg-card) 45%, transparent);
    pointer-events: none;
  }

  .header-copy,
  .title-copy,
  .technology-group {
    display: grid;
    min-width: 0;
  }

  .header-copy {
    gap: var(--space-3);
    padding-inline-end: 2.75rem;
  }

  .title-copy,
  .technology-group {
    gap: var(--space-2);
  }

  .technology-group {
    align-content: start;
  }

  :global(.platform-row),
  :global(.technology-row) {
    justify-content: flex-start;
  }

  .game-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
    line-height: 1.2;
  }

  .field-label {
    margin: 0 0 var(--space-1);
    color: var(--text-subtle);
    font-size: 0.6875rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .card-path {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.8125rem;
    line-height: 1.4;
    word-break: break-word;
  }

  :global(.platform-row) :global(.badge) {
    max-width: min(100%, 15rem);
  }

  .card-actions {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }

  @media (max-width: 760px) {
    .overview-bar {
      align-items: flex-start;
    }

    .overview-bar .action-group {
      margin-inline: 0;
    }
  }

  @media (max-width: 720px) {
    .card-header {
      grid-template-columns: 1fr;
      gap: 0.9rem;
    }

    .cover-stack {
      width: min(7.125rem, 40vw);
      justify-self: start;
    }

    .cover-placeholder span {
      font-size: 1.2rem;
    }

    :global(.dashboard-summary) {
      gap: 0.35rem;
    }

    .card-body :global(button) {
      width: 100%;
    }

    .card-actions {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 560px) {
    :global(.game-card) {
      padding: 0.9rem;
    }
  }
</style>
