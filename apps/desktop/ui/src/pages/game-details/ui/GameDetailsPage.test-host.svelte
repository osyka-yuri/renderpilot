<script lang="ts">
  import type { ComponentProps } from 'svelte';

  import { TooltipProvider } from '@shared/ui';

  import GameDetailsPage from './GameDetailsPage.svelte';

  const props: ComponentProps<typeof GameDetailsPage> = $props();
</script>

<TooltipProvider delayDuration={0}>
  <GameDetailsPage {...props} />
</TooltipProvider>
