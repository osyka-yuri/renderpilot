<script lang="ts">
  import { Badge } from '@shared/ui';
  import { addonCapabilityLabel } from '../model/addon-capabilities';
  import type { AddonCapability } from '../model/types';

  type Props = {
    addons?: readonly AddonCapability[];
  };

  const EMPTY_ADDONS: readonly AddonCapability[] = [];

  let { addons = EMPTY_ADDONS }: Props = $props();
</script>

<div class="flex flex-wrap gap-1.5">
  {#each addons as addon (addon)}
    <Badge variant="outline">{addonCapabilityLabel(addon)}</Badge>
  {/each}
</div>
