<script lang="ts">
  import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';
  import { fileNameFromPath } from '@shared/path';
  import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
    ItemGroup,
    ItemSeparator,
  } from '@shared/ui';
  import ComponentVersionRow from './ComponentVersionRow.svelte';

  type Props = {
    components: GameGraphicsComponent[];
    groupsById: Record<string, GameCandidateGroup | null>;
    busy: boolean;
    onSwap: (componentId: string, artifactId: string, entryId: string | null) => void;
    onRollback: (componentId: string) => void;
  };

  const { components, groupsById, busy, onSwap, onRollback }: Props = $props();

  // Sort by file name so the row order is stable across re-renders and
  // matches NVIDIA's own canonical Streamline plugin ordering reasonably
  // well (sl.common.dll, sl.dlss.dll, sl.dlss_d.dll, sl.dlss_g.dll, ...).
  const orderedComponents = $derived(
    [...components].sort((a, b) => {
      const aName = fileNameFromPath(a.files[0]?.path ?? '');
      const bName = fileNameFromPath(b.files[0]?.path ?? '');
      return aName.localeCompare(bName);
    }),
  );
</script>

<Card>
  <CardHeader class="pb-2">
    <CardTitle>NVIDIA Streamline</CardTitle>
    <CardDescription>
      Multi-plugin framework. Each plugin DLL ships and updates independently — manage them all in
      one place.
    </CardDescription>
  </CardHeader>

  <CardContent>
    <ItemGroup class="rounded-md border bg-muted/30">
      {#each orderedComponents as component, index (component.id)}
        {@const group = groupsById[component.id] ?? null}

        {#if index > 0}
          <ItemSeparator />
        {/if}

        <ComponentVersionRow {component} {group} {busy} {onSwap} {onRollback} />
      {/each}
    </ItemGroup>
  </CardContent>
</Card>
