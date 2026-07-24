<script lang="ts">
  import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';
  import { TooltipProvider } from '@shared/ui';
  import type { SwapHandler } from '../model/create-game-details-page-model';

  import ComponentVersionRow from './ComponentVersionRow.svelte';

  type Props = {
    component: GameGraphicsComponent;
    group: GameCandidateGroup | null;
    busy: boolean;
    onSwap: SwapHandler;
    onRollback: (componentId: string) => void;
  };

  const { component, group, busy, onSwap, onRollback }: Props = $props();
</script>

<TooltipProvider>
  <ComponentVersionRow {component} {group} {busy} {onSwap} {onRollback} />
</TooltipProvider>
