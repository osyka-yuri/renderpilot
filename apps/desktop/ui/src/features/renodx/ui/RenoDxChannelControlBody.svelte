<script lang="ts">
  import RenoDxChannelToggleGroup from './RenoDxChannelToggleGroup.svelte';

  import type { ReshadeChannel } from '@entities/addon';

  type Props = {
    value: ReshadeChannel;
    stableSupported: boolean;
    busy: boolean;
    ariaLabel: string;
    describedBy: string | undefined;
    tooltipDescriptionId: string | undefined;
    tooltipText: string | undefined;
    onChange: (channel: ReshadeChannel) => void;
  };

  const {
    value,
    stableSupported,
    busy,
    ariaLabel,
    describedBy,
    tooltipDescriptionId,
    tooltipText,
    onChange,
  }: Props = $props();
</script>

<RenoDxChannelToggleGroup
  {value}
  {stableSupported}
  {busy}
  {ariaLabel}
  {describedBy}
  {tooltipText}
  {onChange}
/>

{#if tooltipText}
  <span id={tooltipDescriptionId} class="sr-only">{tooltipText}</span>
{/if}
