<script lang="ts">
  import { Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import RenoDxChannelToggleGroup from './RenoDxChannelToggleGroup.svelte';

  import type { ReshadeChannel } from '../model/types';

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

{#if tooltipText}
  <Tooltip>
    <TooltipTrigger>
      <RenoDxChannelToggleGroup
        {value}
        {stableSupported}
        {busy}
        {ariaLabel}
        {describedBy}
        {onChange}
      />
    </TooltipTrigger>

    <TooltipContent>{tooltipText}</TooltipContent>
  </Tooltip>
{:else}
  <RenoDxChannelToggleGroup {value} {stableSupported} {busy} {ariaLabel} {describedBy} {onChange} />
{/if}

{#if tooltipText}
  <span id={tooltipDescriptionId} class="sr-only">{tooltipText}</span>
{/if}
