<script lang="ts">
  import {
    SegmentedControl,
    SegmentedControlItem,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';
  import { t, type MessageKeyWithoutParams } from '@shared/i18n';
  import { isReshadeChannel, type ReshadeChannel } from '@entities/addon';

  type Props = {
    value: ReshadeChannel;
    stableSupported: boolean;
    busy?: boolean;
    ariaLabel: string;
    describedBy?: string;
    tooltipText?: string;
    onChange: (channel: ReshadeChannel) => void;
  };

  type ChannelOption = {
    value: ReshadeChannel;
    labelKey: MessageKeyWithoutParams;
  };

  const CHANNEL_OPTIONS = [
    {
      value: 'stable',
      labelKey: 'gameDetails.renodx.channel.stable',
    },
    {
      value: 'nightly',
      labelKey: 'gameDetails.renodx.channel.nightly',
    },
  ] as const satisfies readonly ChannelOption[];

  const {
    value,
    stableSupported,
    busy = false,
    ariaLabel,
    describedBy,
    tooltipText,
    onChange,
  }: Props = $props();

  function isChannelSupported(channel: ReshadeChannel): boolean {
    return channel !== 'stable' || stableSupported;
  }

  function isChannelDisabled(channel: ReshadeChannel): boolean {
    return busy || !isChannelSupported(channel);
  }

  function canCommitChannelChange(channel: ReshadeChannel): boolean {
    return channel !== value && !busy && isChannelSupported(channel);
  }

  function handleValueChange(next: unknown): void {
    if (!isReshadeChannel(next) || !canCommitChannelChange(next)) {
      return;
    }

    onChange(next);
  }
</script>

<SegmentedControl
  {value}
  disabled={busy}
  onValueChange={handleValueChange}
  aria-label={ariaLabel}
  aria-describedby={describedBy}
>
  {#each CHANNEL_OPTIONS as option (option.value)}
    {#if tooltipText}
      <Tooltip>
        <TooltipTrigger>
          {#snippet child({ props })}
            <SegmentedControlItem
              {...props}
              value={option.value}
              disabled={isChannelDisabled(option.value)}
            >
              {t(option.labelKey)}
            </SegmentedControlItem>
          {/snippet}
        </TooltipTrigger>
        <TooltipContent>{tooltipText}</TooltipContent>
      </Tooltip>
    {:else}
      <SegmentedControlItem value={option.value} disabled={isChannelDisabled(option.value)}>
        {t(option.labelKey)}
      </SegmentedControlItem>
    {/if}
  {/each}
</SegmentedControl>
