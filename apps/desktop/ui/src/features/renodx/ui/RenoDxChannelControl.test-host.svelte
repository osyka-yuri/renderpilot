<script lang="ts">
  import { TooltipProvider } from '@shared/ui';
  import RenoDxChannelControl from './RenoDxChannelControl.svelte';

  type Props = {
    title?: string | null;
  };

  const { title = 'Choose the release channel.' }: Props = $props();
  const controlLabel = 'Release channel';
</script>

<TooltipProvider delayDuration={0}>
  <RenoDxChannelControl
    value="stable"
    stableSupported
    label={controlLabel}
    ariaLabel={controlLabel}
    {title}
    onChange={() => undefined}
  />
</TooltipProvider>
