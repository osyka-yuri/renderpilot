<script lang="ts">
  import type { ComponentProps } from 'svelte';

  import type { GameDetails } from '@entities/game';
  import { TooltipProvider } from '@shared/ui';

  import GameDetailsPage from './GameDetailsPage.svelte';

  const props: ComponentProps<typeof GameDetailsPage> = $props();
  let detailsOverridden = $state(false);
  let detailsOverride = $state<GameDetails | null>(null);

  export function replaceDetails(details: GameDetails | null): void {
    detailsOverridden = true;
    detailsOverride = details;
  }
</script>

<TooltipProvider delayDuration={0}>
  <GameDetailsPage {...props} details={detailsOverridden ? detailsOverride : props.details} />
</TooltipProvider>
