<script lang="ts">
  import type { ComponentProps } from 'svelte';
  import { TooltipProvider } from '@shared/ui';

  import GameExecutablePopover from './GameExecutablePopover.svelte';

  type Props = ComponentProps<typeof GameExecutablePopover>;

  const { ...props }: Props = $props();
</script>

<TooltipProvider delayDuration={0}>
  <GameExecutablePopover {...props} />
</TooltipProvider>
