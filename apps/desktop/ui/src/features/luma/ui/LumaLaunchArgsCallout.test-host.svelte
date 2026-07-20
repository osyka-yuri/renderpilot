<script lang="ts">
  import { TooltipProvider } from '@shared/ui';

  import LumaLaunchArgsCallout from './LumaLaunchArgsCallout.svelte';

  type Props = {
    launchArgs: string[];
    launcher: string;
  };

  const { launchArgs, launcher }: Props = $props();
</script>

<TooltipProvider>
  <LumaLaunchArgsCallout {launchArgs} {launcher} />
</TooltipProvider>
