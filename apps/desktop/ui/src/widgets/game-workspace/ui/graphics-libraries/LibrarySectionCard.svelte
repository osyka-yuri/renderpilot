<script lang="ts">
  import type { LibrarySection } from '@features/graphics-configurator';
  import { Badge, Card, CardContent, CardHeader, CardTitle } from '@shared/ui';
  import ComponentSwapRow from './ComponentSwapRow.svelte';
  import NvapiDriverControls from './NvapiDriverControls.svelte';

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

<article>
  <Card>
    <CardHeader>
      <div class="flex items-start justify-between gap-4 max-lg:flex-col max-lg:items-start">
        <div class="grid gap-1">
          <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
            {eyebrow}
          </p>
          <CardTitle>{section.label}</CardTitle>
        </div>

        <div class="flex flex-wrap gap-2">
          <Badge
            >{section.rows.length} detected {section.rows.length === 1 ? 'file' : 'files'}</Badge
          >
          {#if section.totalCandidates > 0}
            <Badge
              >{section.totalCandidates} replacement {section.totalCandidates === 1
                ? 'version'
                : 'versions'}</Badge
            >
          {:else}
            <Badge variant="secondary">No replacements</Badge>
          {/if}
        </div>
      </div>
    </CardHeader>

    <CardContent>
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
    </CardContent>
  </Card>
</article>
