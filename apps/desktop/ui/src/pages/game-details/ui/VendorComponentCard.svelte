<script lang="ts">
  import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';
  import { formatCanonicalLibraryLabel } from '@shared/graphics';
  import { Card, CardContent, CardDescription, CardHeader, CardTitle, ItemGroup } from '@shared/ui';
  import ComponentVersionRow from './ComponentVersionRow.svelte';

  type Props = {
    component: GameGraphicsComponent;
    group: GameCandidateGroup | null;
    busy: boolean;
    onSwap: (componentId: string, artifactId: string, entryId: string | null) => void;
    onRollback: (componentId: string) => void;
  };

  const { component, group, busy, onSwap, onRollback }: Props = $props();

  const title = $derived(formatCanonicalLibraryLabel(component.technology));
</script>

<Card>
  <CardHeader class="pb-2">
    <CardTitle>{title}</CardTitle>
    <CardDescription>Swap the on-disk library file to a different version.</CardDescription>
  </CardHeader>

  <CardContent>
    <ItemGroup class="rounded-md border bg-muted/30">
      <ComponentVersionRow {component} {group} {busy} {onSwap} {onRollback} />
    </ItemGroup>
  </CardContent>
</Card>
