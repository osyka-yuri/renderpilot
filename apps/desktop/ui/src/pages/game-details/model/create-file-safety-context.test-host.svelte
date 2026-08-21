<script lang="ts">
  import { onDestroy, untrack } from 'svelte';

  import type { GameFileSafetyAssessment } from '@entities/game';

  import {
    createFileSafetyContext,
    type FileSafetyScope,
  } from './create-file-safety-context.svelte';

  let { initialGameId }: { initialGameId: string } = $props();
  let gameId = $state<string | null>(untrack(() => initialGameId));
  const context = createFileSafetyContext({ getGameId: () => gameId });

  export function replaceGameId(nextGameId: string): void {
    gameId = nextGameId;
  }

  export function requireTokens(scope: FileSafetyScope) {
    return context.requireTokens(scope);
  }

  export function getAssessment(): GameFileSafetyAssessment | null {
    return context.assessment;
  }

  onDestroy(context.destroy);
</script>
