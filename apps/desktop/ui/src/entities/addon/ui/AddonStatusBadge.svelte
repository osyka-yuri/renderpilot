<script lang="ts">
  import CircleArrowUpIcon from '@lucide/svelte/icons/circle-arrow-up';
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import InfoIcon from '@lucide/svelte/icons/info';
  import { Badge, Spinner } from '@shared/ui';

  import { ICON_BY_STATUS, TINT_BY_STATUS, type AddonBadgeStatus } from '../model/badge-status';

  export type { AddonBadgeStatus } from '../model/badge-status';

  type Props = {
    status: AddonBadgeStatus;
    /** Already-translated label for this status. */
    label: string;
  };

  const { status, label }: Props = $props();

  const icon = $derived(ICON_BY_STATUS[status]);
  const tint = $derived(TINT_BY_STATUS[status]);
</script>

<Badge variant="outline" class={tint}>
  {#if icon === 'success'}
    <CircleCheckIcon aria-hidden="true" />
  {:else if icon === 'update'}
    <CircleArrowUpIcon aria-hidden="true" />
  {:else if icon === 'checking'}
    <Spinner class="size-3" />
  {:else}
    <InfoIcon aria-hidden="true" />
  {/if}

  {label}
</Badge>
