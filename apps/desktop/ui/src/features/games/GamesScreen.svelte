<script lang="ts">
  import type { GameCard } from '@shared/api/types';
  import type { GameSelectionHandler, VoidHandler } from '@shared/utils/callbacks';
  import Badge from '@shared/ui/Badge.svelte';
  import Button from '@shared/ui/Button.svelte';
  import Surface from '@shared/ui/Surface.svelte';
  import { formatLabel, titleMonogram } from '@shared/utils/presenters';

  type DashboardStats = {
    games: number;
    updates: number;
    backupsReady: number;
  };

  type UpdateBadgeTone = 'success' | 'muted';

  const SCAN_LABEL = 'Scan Folder';
  const SCANNING_LABEL = 'Scanning...';

  const noop: VoidHandler = (): void => {
    return;
  };
  const noopOpenGame: GameSelectionHandler = (_gameId: string): void => {
    return;
  };

  export let games: GameCard[] = [];
  export let busy = false;
  export let onScan: VoidHandler = noop;
  export let onOpenDetails: GameSelectionHandler = noopOpenGame;
  export let onOpenOperations: GameSelectionHandler = noopOpenGame;

  $: hasGames = games.length > 0;
  $: scanButtonLabel = busy ? SCANNING_LABEL : SCAN_LABEL;
  $: dashboardStats = getDashboardStats(games);

  function getDashboardStats(gameCards: GameCard[]): DashboardStats {
    return gameCards.reduce<DashboardStats>(
      (stats, game) => ({
        games: stats.games + 1,
        updates: stats.updates + getUpdateCount(game),
        backupsReady: stats.backupsReady + Number(game.backup_available),
      }),
      {
        games: 0,
        updates: 0,
        backupsReady: 0,
      },
    );
  }

  function getUpdateCount(game: GameCard): number {
    return Math.max(0, game.update_count);
  }

  function getUpdateBadgeTone(game: GameCard): UpdateBadgeTone {
    return game.updates_available ? 'success' : 'muted';
  }

  function getUpdateBadgeLabel(game: GameCard): string {
    if (!game.updates_available) {
      return 'Up to date';
    }

    const updateCount = getUpdateCount(game);

    if (updateCount === 0) {
      return 'Updates available';
    }

    return `${updateCount} update${updateCount === 1 ? '' : 's'} available`;
  }

  function getGameIdFromEvent(event: MouseEvent): string | null {
    const { currentTarget } = event;

    if (!(currentTarget instanceof HTMLElement)) {
      return null;
    }

    return currentTarget.dataset.gameId ?? null;
  }

  function handleScan(): void {
    onScan();
  }

  function handleDetailsClick(event: MouseEvent): void {
    const gameId = getGameIdFromEvent(event);

    if (gameId === null) {
      return;
    }

    onOpenDetails(gameId);
  }

  function handleOperationsClick(event: MouseEvent): void {
    const gameId = getGameIdFromEvent(event);

    if (gameId === null) {
      return;
    }

    onOpenOperations(gameId);
  }
</script>

<section class="screen-shell" aria-busy={busy}>
  <div class="overview-bar">
    {#if hasGames}
      <div class="overview-stats" aria-label="Dashboard summary">
        <Badge pill surface="outline">{dashboardStats.games} games</Badge>
        <Badge pill surface="outline">{dashboardStats.updates} updates</Badge>

        {#if dashboardStats.backupsReady > 0}
          <Badge pill surface="outline" tone="success">
            {dashboardStats.backupsReady} backup-ready
          </Badge>
        {/if}
      </div>
    {/if}

    <div class="overview-action">
      <Button variant="primary" size="sm" disabled={busy} loading={busy} onclick={handleScan}>
        {scanButtonLabel}
      </Button>
    </div>
  </div>

  {#if !hasGames}
    <Surface class="empty-state">
      <div class="empty-icon" aria-hidden="true">RP</div>

      <div class="empty-copy">
        <h3>No scanned games yet</h3>
        <p>
          Select a game folder to populate the dashboard with components, updates, backup state, and
          quick actions.
        </p>
      </div>

      <Button variant="primary" size="sm" disabled={busy} loading={busy} onclick={handleScan}>
        {scanButtonLabel}
      </Button>
    </Surface>
  {:else}
    <div class="game-list">
      {#each games as game (game.game_id)}
        <Surface as="article" interactive shadow class="game-card">
          <div class="card-body">
            <div class="card-header">
              <div aria-hidden="true" class="cover-placeholder">
                <span>{titleMonogram(game.title)}</span>
              </div>

              <div class="header-copy">
                <div class="platform-row">
                  <Badge pill surface="soft" tone={getUpdateBadgeTone(game)} class="updates-badge">
                    {getUpdateBadgeLabel(game)}
                  </Badge>
                </div>

                <div class="title-copy">
                  <h3>{game.title}</h3>
                  <p class="card-path">{game.install_path}</p>
                </div>
              </div>
            </div>

            <div class="technology-group">
              <p class="field-label">Detected libraries</p>

              <div class="technology-row">
                {#if game.technology_tags.length === 0}
                  <Badge pill surface="outline" tone="muted">No detected technologies yet</Badge>
                {:else}
                  {#each game.technology_tags as technology}
                    <Badge pill surface="outline">{formatLabel(technology)}</Badge>
                  {/each}
                {/if}
              </div>
            </div>

            <div class="card-actions">
              <Button
                variant="primary"
                size="sm"
                fullWidth
                data-game-id={game.game_id}
                aria-label={`Open details for ${game.title}`}
                onclick={handleDetailsClick}
              >
                Details
              </Button>

              <Button
                variant="secondary"
                size="sm"
                fullWidth
                data-game-id={game.game_id}
                aria-label={`Open journal for ${game.title}`}
                onclick={handleOperationsClick}
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

  .overview-stats {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .overview-action {
    margin-left: auto;
  }

  h3 {
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
    overflow: hidden;
  }

  .card-body {
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

  .cover-placeholder {
    display: grid;
    min-height: 4.75rem;
    align-content: center;
    justify-items: center;
    border: 1px solid color-mix(in srgb, var(--accent-outline) 48%, var(--border-subtle));
    border-radius: var(--radius-lg);
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--accent) 16%, var(--bg-control)) 0%,
      var(--bg-control) 100%
    );
    color: var(--text-strong);
    box-shadow: inset 0 1px 0 color-mix(in srgb, white 10%, transparent);
  }

  .cover-placeholder span {
    font-size: 1.45rem;
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .header-copy,
  .title-copy,
  .technology-group {
    display: grid;
    min-width: 0;
  }

  .header-copy {
    gap: var(--space-3);
  }

  .title-copy {
    gap: var(--space-2);
  }

  .technology-group {
    gap: var(--space-2);
  }

  .platform-row,
  .technology-row {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 0.4rem;
  }

  :global(.updates-badge) {
    flex-shrink: 0;
    align-self: start;
    max-width: min(100%, 15rem);
    line-height: 1.2;
    text-align: center;
    white-space: normal;
  }

  .card-path {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.8125rem;
    line-height: 1.4;
    word-break: break-word;
  }

  .card-actions {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }

  :global(.empty-state) {
    display: grid;
    justify-items: start;
    gap: var(--space-3);
    padding: var(--space-6);
    border-style: dashed;
    background: color-mix(in srgb, var(--bg-card) 62%, transparent);
  }

  .empty-icon {
    display: grid;
    width: 2.5rem;
    height: 2.5rem;
    place-items: center;
    border-radius: var(--radius-lg);
    background: var(--accent-soft);
    color: var(--accent-strong);
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  .empty-copy {
    display: grid;
    gap: var(--space-1);
    max-width: 36rem;
  }

  .empty-copy p {
    margin: 0;
  }

  @media (max-width: 760px) {
    .overview-bar {
      align-items: flex-start;
    }

    .overview-action {
      margin-inline: 0;
    }
  }

  @media (max-width: 720px) {
    .card-header {
      grid-template-columns: 1fr;
      gap: 0.9rem;
    }

    .cover-placeholder {
      width: 72px;
      min-height: 72px;
    }

    .overview-stats {
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
    .overview-action,
    .overview-action :global(button) {
      width: 100%;
    }

    :global(.game-card) {
      padding: 0.9rem;
    }
  }
</style>
