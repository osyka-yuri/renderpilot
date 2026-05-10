<script lang="ts">
  import type { GameCard, GameDetails } from '@shared/api/types';

  type Props = {
    details?: GameDetails | null;
    gameCard?: GameCard | null;
  };

  let { details = null, gameCard = null }: Props = $props();

  const backupCoverage = $derived(
    details?.operations.filter((operation) => operation.backup_count > 0).length ?? 0,
  );
</script>

<section class="screen-shell">
  <section class="hero-panel">
    <div>
      <p class="eyebrow">Backups</p>
      <h2>Rollback coverage and snapshot readiness</h2>
      <p>
        The dedicated backup browser will list manifests, timestamps, libraries, and restore
        targets. Until that endpoint exists, this screen keeps the final product shape and reports
        available coverage from the operation journal.
      </p>
    </div>

    <div class="hero-side">
      <article>
        <span>Focused game</span>
        <strong>{gameCard?.title ?? 'No focused game selected'}</strong>
      </article>
      <article>
        <span>Journal entries with backups</span>
        <strong>{backupCoverage}</strong>
      </article>
    </div>
  </section>

  <section class="coverage-grid">
    <article class="coverage-card">
      <p class="eyebrow">Manifest</p>
      <h3>Snapshot metadata</h3>
      <p>Operation ID, original path, library, hash, and app version land here.</p>
    </article>
    <article class="coverage-card">
      <p class="eyebrow">Restore</p>
      <h3>Single component rollback</h3>
      <p>Per-component restore actions will appear once a dedicated backups endpoint exists.</p>
    </article>
    <article class="coverage-card">
      <p class="eyebrow">Recovery</p>
      <h3>Crash safety</h3>
      <p>Incomplete operations and recovery warnings belong on this surface.</p>
    </article>
  </section>
</section>

<style>
  .screen-shell {
    display: grid;
    gap: 1rem;
  }

  .hero-panel,
  .coverage-card {
    border-radius: 1.6rem;
    border: 1px solid var(--border-subtle);
    background: var(--bg-panel);
  }

  .hero-panel {
    display: grid;
    grid-template-columns: minmax(0, 1.3fr) minmax(250px, 0.7fr);
    gap: 1rem;
    padding: 1.25rem;
  }

  .hero-side {
    display: grid;
    gap: 0.8rem;
  }

  .hero-side article,
  .coverage-card {
    padding: 1rem;
    border-radius: 1.2rem;
    background: var(--bg-soft);
  }

  .coverage-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.9rem;
  }

  .eyebrow,
  span {
    display: block;
    margin-bottom: 0.35rem;
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-size: 0.72rem;
  }

  h2,
  h3,
  strong {
    margin: 0;
  }

  @media (max-width: 900px) {
    .hero-panel,
    .coverage-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
