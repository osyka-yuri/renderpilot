<script lang="ts">
  import type { LibrarySection } from '@features/graphics-configurator';
  import { Badge, BadgeGroup, SectionHeader, Surface } from '@shared/ui';
  import ComponentSwapRow from './ComponentSwapRow.svelte';
  import NvapiDriverControls from './NvapiDriverControls.svelte';
  import { cn } from '@shared/utils';

  type Props = {
    section: LibrarySection;
    eyebrow?: string;
    selectedArtifacts?: Record<string, string>;
    selectedNvapiSelections?: Record<string, string>;
    riskLevel?: string | null | undefined;
    busy?: boolean;
    selectionKey: (componentId: string, controlId: string) => string;
    onArtifactSelection: (componentId: string, value: string) => void;
    onNvapiSelection: (componentId: string, controlId: string, value: string) => void;
    onBuildPlan: (componentId: string, artifactId: string) => void;
  };

  const {
    section,
    eyebrow = 'Library',
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

<Surface as="article" tone="panel" shadow class={cn('grid gap-4 p-4', 'max-lg:p-3')}>
  <SectionHeader
    {eyebrow}
    title={section.label}
    titleTag="h4"
    class={cn(
      'flex items-start justify-between gap-4 border-b border-border-subtle pb-3',
      'max-lg:flex-col max-lg:items-start',
    )}
  >
    <BadgeGroup>
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
  </SectionHeader>

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

  <div class="grid gap-3">
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
</Surface>
