<script lang="ts">
  import { t } from '@shared/i18n';
  import { ToggleGroup, ToggleGroupItem } from '@shared/ui';

  import type { ReshadeChannel } from '../model/types';
  import RenoDxFieldLabel from './RenoDxFieldLabel.svelte';

  type Props = {
    value: ReshadeChannel;
    stableSupported: boolean;
    disabled?: boolean;
    onValueChange?: (value: ReshadeChannel) => void;
  };

  const {
    value,
    stableSupported,
    disabled = false,
    onValueChange = () => undefined,
  }: Props = $props();

  const stableDisabled = $derived(disabled || !stableSupported);
  const stableTitle = $derived(
    stableSupported
      ? t('gameDetails.renodx.channel.stable')
      : t('gameDetails.renodx.channel.stableUnavailable'),
  );

  function handleValueChange(next: string | null): void {
    if (next === 'stable' || next === 'nightly') {
      onValueChange(next);
    }
  }
</script>

<RenoDxFieldLabel label={t('gameDetails.renodx.channel.label')} inline={false}>
  <ToggleGroup
    type="single"
    variant="outline"
    size="sm"
    {value}
    onValueChange={handleValueChange}
    aria-label={t('gameDetails.renodx.channel.label')}
  >
    <ToggleGroupItem value="stable" disabled={stableDisabled} title={stableTitle}>
      {t('gameDetails.renodx.channel.stable')}
    </ToggleGroupItem>
    <ToggleGroupItem value="nightly" {disabled}>
      {t('gameDetails.renodx.channel.nightly')}
    </ToggleGroupItem>
  </ToggleGroup>
</RenoDxFieldLabel>
