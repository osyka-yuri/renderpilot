<script lang="ts">
  import type { LibrarySection } from '@features/graphics-configurator';
  import { Badge, BadgeGroup } from '@shared/ui';
  import ComponentSwapRow from './ComponentSwapRow.svelte';
  import NvapiDriverControls from './NvapiDriverControls.svelte';

  type Props = {
    section: LibrarySection;
    selectedArtifacts?: Record<string, string>;
    selectedNvapiSelections?: Record<string, string>;
    riskLevel?: string | null | undefined;
    busy?: boolean;
    selectionKey: (componentId: string, controlId: string) => string;
    onArtifactSelection: (componentId: string, value: string) => void;
    onNvapiSelection: (componentId: string, controlId: string, value: string) => void;
    onBuildPlan: (componentId: string, artifactId: string) => void;
  };

  let {
    section,
    selectedArtifacts = {},
    selectedNvapiSelections = {},
    riskLevel = null,
    busy = false,
    selectionKey,
    onArtifactSelection,
    onNvapiSelection,
    onBuildPlan,
  }: Props = $props();
</script>

<article class="library-card">
  <div class="library-head">
    <div>
      <p class="eyebrow">Library</p>
      <h4>{section.label}</h4>
    </div>

    <BadgeGroup class="library-badges">
      <Badge>{section.rows.length} detected {section.rows.length === 1 ? 'file' : 'files'}</Badge>
      {#if section.totalCandidates > 0}
        <Badge
          >{section.totalCandidates} replacement {section.totalCandidates === 1
            ? 'version'
            : 'versions'}</Badge
        >
      {:else}
        <Badge tone="muted">No replacements</Badge>
      {/if}
    </BadgeGroup>
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

  <div class="library-rows">
    {#each section.rows as row (row.component.id)}
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
    background: var(--bg-panel);
    box-shadow: var(--shadow-card);
  }

  .library-head {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: flex-start;
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

  .library-rows {
    display: grid;
    gap: var(--space-3);
  }

  @media (max-width: 820px) {
    .library-card {
      padding: var(--space-3);
    }

    .library-head {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
