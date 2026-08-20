<script lang="ts">
  import {
    ToggleGroup,
    ToggleGroupItem,
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

<ToggleGroup
  type="single"
  variant="outline"
  spacing={0}
  {value}
  disabled={busy}
  onValueChange={handleValueChange}
  class="w-fit"
  aria-label={ariaLabel}
  aria-describedby={describedBy}
>
  {#each CHANNEL_OPTIONS as option (option.value)}
    {#if tooltipText}
      <Tooltip>
        <TooltipTrigger>
          {#snippet child({ props })}
            <ToggleGroupItem
              {...props}
              value={option.value}
              disabled={isChannelDisabled(option.value)}
            >
              {t(option.labelKey)}
            </ToggleGroupItem>
          {/snippet}
        </TooltipTrigger>
        <TooltipContent>{tooltipText}</TooltipContent>
      </Tooltip>
    {:else}
      <ToggleGroupItem value={option.value} disabled={isChannelDisabled(option.value)}>
        {t(option.labelKey)}
      </ToggleGroupItem>
    {/if}
  {/each}
</ToggleGroup>
