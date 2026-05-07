<script lang="ts">
  import type { GameCard } from '@shared/api/types';
  import type { GameSelectionHandler, VoidHandler } from '@shared/utils/callbacks';
  import Badge from '@shared/ui/Badge.svelte';
  import Button from '@shared/ui/Button.svelte';
  import Surface from '@shared/ui/Surface.svelte';
  import { formatLabel, titleMonogram } from '@shared/utils/presenters';

  const noop: VoidHandler = (): void => {};
  const noopOpenGame: GameSelectionHandler = (_gameId: string): void => {};

  export let games: GameCard[] = [];
  export let busy = false;
  export let onScan: VoidHandler = noop;
  export let onOpenDetails: GameSelectionHandler = noopOpenGame;
  export let onOpenOperations: GameSelectionHandler = noopOpenGame;

  $: dashboardStats = {
    games: games.length,
    updates: games.reduce((sum, game) => sum + game.update_count, 0),
    backupsReady: games.filter((game) => game.backup_available).length,
  };

  function handleScan(): void {
    onScan();
  }

  function handleOpenDetails(gameId: string): void {
    onOpenDetails(gameId);
  }

  function handleOpenOperations(gameId: string): void {
    onOpenOperations(gameId);
  }
</script>

<section class="screen-shell">
  <div class="overview-bar">
    {#if games.length > 0}
      <div class="overview-stats">
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
        {busy ? 'Scanning...' : 'Scan Folder'}
      </Button>
    </div>
  </div>

  {#if games.length === 0}
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
        {busy ? 'Scanning...' : 'Scan Folder'}
      </Button>
    </Surface>
  {:else}
    <div class="game-list">
      {#each games as game}
        <Surface as="article" interactive shadow class="game-card">
          <div class="card-body">
            <div class="card-header">
              <div aria-hidden="true" class="cover-placeholder">
                <span>{titleMonogram(game.title)}</span>
              </div>

              <div class="header-copy">
                <div class="platform-row">
                  <Badge
                    pill
                    surface="soft"
                    tone={game.updates_available ? 'success' : 'muted'}
                    class="updates-badge"
                  >
                    {#if game.updates_available}
                      {game.update_count} update{game.update_count === 1 ? '' : 's'} available
                    {:else}
                      Up to date
                    {/if}
                  </Badge>
                </div>

                <div class="title-row">
                  <div class="title-copy">
                    <h3>{game.title}</h3>
                    <p class="card-path">{game.install_path}</p>
                  </div>
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
                onclick={() => handleOpenDetails(game.game_id)}
              >
                Details
              </Button>
              <Button
                variant="secondary"
                size="sm"
                fullWidth
                onclick={() => handleOpenOperations(game.game_id)}
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
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3) var(--space-4);
    flex-wrap: wrap;
    padding: 0 var(--space-1);
  }

  .overview-stats {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .field-label {
    margin: 0 0 var(--space-1);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.6875rem;
    color: var(--text-subtle);
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

  .game-list {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(20.5rem, 1fr));
    gap: var(--space-3);
    align-items: stretch;
  }

  :global(.game-card) {
    position: relative;
    display: grid;
    height: 100%;
    min-width: 0;
    padding: var(--space-4);
    overflow: hidden;
  }

  .card-body {
    display: grid;
    grid-template-rows: auto 1fr auto;
    gap: var(--space-4);
    height: 100%;
    min-width: 0;
  }

  .card-header {
    display: grid;
    grid-template-columns: 4.75rem minmax(0, 1fr);
    gap: var(--space-3);
    align-items: start;
  }

  .cover-placeholder {
    min-height: 4.75rem;
    border-radius: var(--radius-lg);
    display: grid;
    align-content: center;
    justify-items: center;
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--accent) 16%, var(--bg-control)) 0%,
      var(--bg-control) 100%
    );
    color: var(--text-strong);
    border: 1px solid color-mix(in srgb, var(--accent-outline) 48%, var(--border-subtle));
    box-shadow: inset 0 1px 0 color-mix(in srgb, white 10%, transparent);
  }

  .cover-placeholder span {
    font-size: 1.45rem;
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .header-copy {
    display: grid;
    gap: var(--space-3);
    min-width: 0;
  }

  .title-row {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    align-items: start;
  }

  .title-copy {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
  }

  :global(.updates-badge) {
    flex-shrink: 0;
    align-self: start;
    max-width: min(100%, 15rem);
    white-space: normal;
    line-height: 1.2;
    text-align: center;
  }

  .platform-row,
  .technology-row {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
    align-items: flex-start;
  }

  .card-path {
    margin: 0;
    color: var(--text-muted);
    word-break: break-word;
    font-size: 0.8125rem;
    line-height: 1.4;
  }

  .technology-group {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
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
    width: 2.5rem;
    height: 2.5rem;
    display: grid;
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
      margin-left: 0;
      margin-right: 0;
    }

    .title-row {
      flex-direction: column;
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
