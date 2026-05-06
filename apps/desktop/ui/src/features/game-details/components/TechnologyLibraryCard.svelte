<script lang="ts">
  import type { TechnologySection } from '@features/game-details/lib/graphics-configurator';
  import Badge from '@shared/ui/Badge.svelte';
  import ComponentSwapRow from './ComponentSwapRow.svelte';
  import NvapiDriverControls from './NvapiDriverControls.svelte';

  export let section: TechnologySection;
  export let selectedArtifacts: Record<string, string> = {};
  export let selectedNvapiSelections: Record<string, string> = {};
  export let riskLevel: string | null | undefined = null;
  export let busy = false;
  export let selectionKey: (componentId: string, controlId: string) => string;
  export let onArtifactSelection: (componentId: string, value: string) => void;
  export let onNvapiSelection: (componentId: string, controlId: string, value: string) => void;
  export let onBuildPlan: (componentId: string, artifactId: string) => void;
</script>

<article class="library-card technology-card">
  <div class="technology-head">
    <div>
      <p class="eyebrow">Technology</p>
      <h4>{section.label}</h4>
    </div>

    <div class="library-badges">
      <Badge>{section.rows.length} detected {section.rows.length === 1 ? 'file' : 'files'}</Badge>
      {#if section.totalCandidates > 0}
        <Badge>{section.totalCandidates} replacement {section.totalCandidates === 1 ? 'version' : 'versions'}</Badge>
      {:else}
        <Badge tone="muted">No replacements</Badge>
      {/if}
    </div>
  </div>

  {#if section.nvapiControls.length > 0}
    <NvapiDriverControls
      controls={section.nvapiControls}
      ownerId={section.nvapiOwnerId}
      selections={selectedNvapiSelections}
      {busy}
      {selectionKey}
      {onNvapiSelection}
    />
  {/if}

  <div class="technology-rows">
    {#each section.rows as row}
      <ComponentSwapRow
        {row}
        selectedArtifact={selectedArtifacts[row.component.id] ?? ''}
        {riskLevel}
        {busy}
        {onArtifactSelection}
        {onBuildPlan}
      />
    {/each}
  </div>
</article>

<style>
  .library-card {
    display: grid;
    gap: var(--space-4);
    padding: var(--space-4);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-xl);
    background:
      linear-gradient(180deg, color-mix(in srgb, var(--bg-card) 96%, white 4%), var(--bg-card));
    box-shadow: var(--shadow-card);
  }

  .technology-card {
    background: var(--bg-panel);
    border-color: var(--border-subtle);
  }

  .technology-head {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
    padding-bottom: var(--space-3);
    border-bottom: 1px solid var(--border-subtle);
  }

  .eyebrow {
    margin: 0 0 0.2rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-subtle);
    font-size: 0.6875rem;
  }

  h4 {
    margin: 0;
    color: var(--text-strong);
    font-size: 1rem;
  }

  .library-badges {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
    margin-top: var(--space-2);
  }

  .technology-rows {
    display: grid;
    gap: var(--space-3);
  }

  @media (max-width: 820px) {
    .library-card {
      padding: var(--space-3);
    }

    .technology-head {
      flex-direction: column;
      align-items: flex-start;
    }

    .library-badges {
      justify-content: flex-start;
    }
  }
</style>
