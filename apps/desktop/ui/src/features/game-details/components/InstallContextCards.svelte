<script lang="ts">
  import { compactList, fileNameFromPath } from '@features/game-details/lib/graphics-configurator';
  import Badge from '@shared/ui/Badge.svelte';

  export let installPath = '';
  export let launchCandidates: string[] = [];
  export let technologies: string[] = [];

  $: launchCandidateNames = launchCandidates.map(fileNameFromPath);
</script>

<section class="install-context" aria-label="Game installation context">
  <div class="install-stack">
    <div class="install-card">
      <span>Folder</span>
      <strong title={installPath}>{installPath}</strong>
    </div>

    <div class="install-card">
      <span>Launch</span>
      <strong title={compactList(launchCandidateNames, 'No executable recorded', 8)}>
        {compactList(launchCandidateNames, 'No executable recorded', 2)}
      </strong>
    </div>
  </div>

  <div class="install-card install-card--graphics">
    <span>Graphics</span>
    <div class="install-technology-badges" title={compactList(technologies, 'No graphics technologies detected', 12)}>
      {#if technologies.length === 0}
        <Badge surface="outline" tone="muted">None detected</Badge>
      {:else}
        {#each technologies as technology}
          <Badge surface="outline">{technology}</Badge>
        {/each}
      {/if}
    </div>
  </div>
</section>

<style>
  .install-context {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
    align-items: stretch;
  }

  .install-stack {
    min-width: 0;
    display: grid;
    gap: var(--space-2);
  }

  .install-card {
    min-width: 0;
    display: grid;
    align-content: start;
    gap: var(--space-2);
    min-height: 4.55rem;
    padding: var(--space-3);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--bg-card) 76%, transparent);
    box-shadow: var(--shadow-card);
  }

  .install-card span {
    color: var(--text-subtle);
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .install-card strong {
    min-width: 0;
    overflow: hidden;
    color: var(--text-strong);
    font-size: 0.84rem;
    font-weight: 600;
    line-height: 1.28;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .install-card--graphics {
    min-height: 100%;
  }

  .install-technology-badges {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    min-width: 0;
  }

  .install-technology-badges :global(.badge) {
    max-width: 100%;
  }

  @media (max-width: 820px) {
    .install-context {
      grid-template-columns: 1fr;
    }
  }
</style>
