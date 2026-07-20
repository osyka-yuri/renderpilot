<script lang="ts">
  import { TooltipProvider } from '@shared/ui';

  import type { LumaGuidance } from '../model/types';

  import LumaGuidanceCallouts from './LumaGuidanceCallouts.svelte';

  type Props = { guidance: LumaGuidance[] };

  const { guidance }: Props = $props();
</script>

<TooltipProvider>
  <LumaGuidanceCallouts {guidance} />
</TooltipProvider>
