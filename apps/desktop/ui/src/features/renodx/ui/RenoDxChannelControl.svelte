<script lang="ts">
  import { t } from '@shared/i18n';
  import RenoDxChannelControlBody from './RenoDxChannelControlBody.svelte';
  import { AddonFieldLabel } from '@entities/addon';

  import type { ReshadeChannel } from '@entities/addon';

  type Props = {
    value: ReshadeChannel;
    stableSupported: boolean;
    busy?: boolean;
    label?: string | null;
    ariaLabel?: string | null;
    title?: string | null;
    class?: string;
    onChange: (channel: ReshadeChannel) => void;
  };

  const {
    value,
    stableSupported,
    busy = false,
    label = null,
    ariaLabel = null,
    title = null,
    class: className,
    onChange,
  }: Props = $props();

  const componentId = $props.id();

  const visibleLabel = $derived(normalizeText(label));
  const tooltipText = $derived(normalizeText(title));

  const tooltipDescriptionId = $derived(
    tooltipText ? `${componentId}-tooltip-description` : undefined,
  );

  const describedBy = $derived(tooltipDescriptionId);

  const fallbackAriaLabel = $derived(t('gameDetails.renodx.channel.label'));

  const toggleGroupAriaLabel = $derived(
    normalizeText(ariaLabel) ?? visibleLabel ?? fallbackAriaLabel,
  );

  function normalizeText(value: string | null | undefined): string | undefined {
    const trimmed = value?.trim();
    return trimmed ?? undefined;
  }
</script>

{#if visibleLabel}
  <AddonFieldLabel label={visibleLabel} class={className}>
    <RenoDxChannelControlBody
      {value}
      {stableSupported}
      {busy}
      ariaLabel={toggleGroupAriaLabel}
      {describedBy}
      {tooltipDescriptionId}
      {tooltipText}
      {onChange}
    />
  </AddonFieldLabel>
{:else}
  <div class={className}>
    <RenoDxChannelControlBody
      {value}
      {stableSupported}
      {busy}
      ariaLabel={toggleGroupAriaLabel}
      {describedBy}
      {tooltipDescriptionId}
      {tooltipText}
      {onChange}
    />
  </div>
{/if}
