<script lang="ts">
  import type { ComponentProps } from 'svelte';

  import { TooltipProvider } from '@shared/ui';

  import RenoDxCard from './RenoDxCard.svelte';

  const props: ComponentProps<typeof RenoDxCard> = $props();
</script>

<TooltipProvider delayDuration={0}>
  <RenoDxCard {...props} />
</TooltipProvider>
