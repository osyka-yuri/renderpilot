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

  type UpdateBadge = {
    label: string;
    tone: UpdateBadgeTone;
  };

  type GameCardViewModel = {
    id: string;
    title: string;
    installPath: string;
    monogram: string;
    updateBadge: UpdateBadge;
    technologies: string[];
  };

  const SCAN_LABEL = 'Scan Folder';
  const SCANNING_LABEL = 'Scanning...';

  const noop: VoidHandler = (): void => {
    // Intentionally empty.
  };

  const noopOpenGame: GameSelectionHandler = (): void => {
    // Intentionally empty.
  };

  export let games: GameCard[] = [];
  export let busy = false;
  export let onScan: VoidHandler = noop;
  export let onRefresh: VoidHandler = noop;
  export let onOpenDetails: GameSelectionHandler = noopOpenGame;
  export let onOpenOperations: GameSelectionHandler = noopOpenGame;

  $: gameItems = games.map(toGameCardViewModel);
  $: hasGames = gameItems.length > 0;
  $: scanButtonLabel = busy ? SCANNING_LABEL : SCAN_LABEL;
  $: dashboardStats = getDashboardStats(games);
  $: hasBackupsReady = dashboardStats.backupsReady > 0;

  function toGameCardViewModel(game: GameCard): GameCardViewModel {
    return {
      id: game.game_id,
      title: game.title,
      installPath: game.install_path,
      monogram: titleMonogram(game.title),
      updateBadge: getUpdateBadge(game),
      technologies: game.technology_tags.map(formatLabel),
    };
  }

  function getDashboardStats(gameCards: readonly GameCard[]): DashboardStats {
    let updates = 0;
    let backupsReady = 0;

    for (const game of gameCards) {
      updates += getUpdateCount(game);
      backupsReady += Number(game.backup_available);
    }

    return {
      games: gameCards.length,
      updates,
      backupsReady,
    };
  }

  function getUpdateBadge(game: GameCard): UpdateBadge {
    return {
      label: getUpdateBadgeLabel(game),
      tone: game.updates_available ? 'success' : 'muted',
    };
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

  function getUpdateCount(game: GameCard): number {
    return Math.max(0, game.update_count);
  }

  function handleScan(): void {
    onScan();
  }

  function handleRefresh(): void {
    onRefresh();
  }
</script>

<section class="screen-shell" aria-busy={busy}>
  <div class="overview-bar">
    {#if hasGames}
      <div class="dashboard-summary" aria-label="Dashboard summary">
        <Badge pill surface="outline">{dashboardStats.games} games</Badge>
        <Badge pill surface="outline">{dashboardStats.updates} updates</Badge>

        {#if hasBackupsReady}
          <Badge pill surface="outline" tone="success">
            {dashboardStats.backupsReady} backup-ready
          </Badge>
        {/if}
      </div>
    {/if}

    <div class="action-group">
      <Button variant="secondary" size="sm" disabled={busy} loading={busy} onclick={handleRefresh}>
        Refresh Libraries
      </Button>

      <Button variant="primary" size="sm" disabled={busy} loading={busy} onclick={handleScan}>
        {scanButtonLabel}
      </Button>
    </div>
  </div>

  {#if !hasGames}
    <Surface class="empty-state">
      <div class="empty-icon" aria-hidden="true">RP</div>

      <div class="empty-copy">
        <h3 class="empty-title">No scanned games yet</h3>
        <p class="empty-description">
          Select a game folder to populate the dashboard with components, updates, backup state,
          and quick actions.
        </p>
      </div>

      <div class="action-group">
        <Button
          variant="secondary"
          size="sm"
          disabled={busy}
          loading={busy}
          onclick={handleRefresh}
        >
          Refresh Libraries
        </Button>

        <Button variant="primary" size="sm" disabled={busy} loading={busy} onclick={handleScan}>
          {scanButtonLabel}
        </Button>
      </div>
    </Surface>
  {:else}
    <div class="game-list">
      {#each gameItems as game (game.id)}
        <Surface as="article" interactive shadow class="game-card">
          <div class="card-body">
            <div class="card-header">
              <div class="cover-placeholder" aria-hidden="true">
                <span>{game.monogram}</span>
              </div>

              <div class="header-copy">
                <div class="platform-row">
                  <Badge
                    pill
                    surface="soft"
                    tone={game.updateBadge.tone}
                    class="updates-badge"
                  >
                    {game.updateBadge.label}
                  </Badge>
                </div>

                <div class="title-copy">
                  <h3 class="game-title">{game.title}</h3>
                  <p class="card-path">{game.installPath}</p>
                </div>
              </div>
            </div>

            <div class="technology-group">
              <p class="field-label">Detected libraries</p>

              <div class="technology-row">
                {#if game.technologies.length === 0}
                  <Badge pill surface="outline" tone="muted">
                    No detected technologies yet
                  </Badge>
                {:else}
                  {#each game.technologies as technology}
                    <Badge pill surface="outline">{technology}</Badge>
                  {/each}
                {/if}
              </div>
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

  .dashboard-summary,
  .action-group,
  .platform-row,
  .technology-row {
    display: flex;
    flex-wrap: wrap;
  }

  .dashboard-summary {
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

  .title-copy,
  .technology-group {
    gap: var(--space-2);
  }

  .platform-row,
  .technology-row {
    align-items: flex-start;
    gap: 0.4rem;
  }

  .game-title,
  .empty-title {
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

  :global(.updates-badge) {
    flex-shrink: 0;
    align-self: start;
    max-width: min(100%, 15rem);
    line-height: 1.2;
    text-align: center;
    white-space: normal;
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

  .empty-description {
    margin: 0;
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

    .cover-placeholder {
      width: 72px;
      min-height: 72px;
    }

    .dashboard-summary {
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
    .action-group {
      width: 100%;
      flex-direction: column-reverse;
    }

    .action-group :global(button) {
      width: 100%;
    }

    :global(.game-card) {
      padding: 0.9rem;
    }
  }
</style>