<script lang="ts">
  import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';
  import { Card, CardContent } from '@shared/ui';
  import ComponentVersionRow from './ComponentVersionRow.svelte';

  type Props = {
    component: GameGraphicsComponent;
    group: GameCandidateGroup | null;
    busy: boolean;
    onSwap: (componentId: string, artifactId: string, entryId: string | null) => void;
    onRollback: (componentId: string) => void;
  };

  const { component, group, busy, onSwap, onRollback }: Props = $props();
</script>

<Card>
  <CardContent class="grid gap-3">
    <div class="rounded-md border bg-muted/30">
      <ComponentVersionRow {component} {group} {busy} {onSwap} {onRollback} />
    </div>
  </CardContent>
</Card>
