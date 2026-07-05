<script lang="ts">
  import { addonCapabilityLabel, type AddonCapability } from '@entities/game';
  import { t } from '@shared/i18n';
  import { ToggleGroup, ToggleGroupItem } from '@shared/ui';

  type Props = {
    options?: readonly AddonCapability[];
    draftAddons?: readonly string[];
    onAddonsChange?: (addons: readonly string[]) => void;
  };

  const HEADING_ID = 'addon-filters-heading';
  const EMPTY_OPTIONS: readonly AddonCapability[] = [];
  const EMPTY_ADDONS: readonly string[] = [];

  let { options = EMPTY_OPTIONS, draftAddons = EMPTY_ADDONS, onAddonsChange }: Props = $props();
</script>

<section class="grid gap-3" aria-labelledby={HEADING_ID}>
  <h3 id={HEADING_ID} class="text-sm font-medium">
    {t('filters.addons.title')}
  </h3>

  <ToggleGroup
    type="multiple"
    variant="outline"
    class="w-full"
    aria-labelledby={HEADING_ID}
    value={[...draftAddons]}
    onValueChange={(nextValue: string[]) => {
      onAddonsChange?.(nextValue);
    }}
  >
    {#each options as option (option)}
      <ToggleGroupItem value={option} class="flex-1" size="sm">
        {addonCapabilityLabel(option)}
      </ToggleGroupItem>
    {/each}
  </ToggleGroup>
</section>
