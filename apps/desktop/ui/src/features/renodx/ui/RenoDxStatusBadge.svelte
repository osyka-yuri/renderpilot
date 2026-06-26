<script lang="ts">
  import { Badge, Spinner } from '@shared/ui';
  import { t } from '@shared/i18n';
  import type { MessageKey } from '@shared/i18n';
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import CircleArrowUpIcon from '@lucide/svelte/icons/circle-arrow-up';
  import InfoIcon from '@lucide/svelte/icons/info';

  import type { RenoDxFreshness } from '../model/types';

  // The shared freshness verdict the card renders as a pill. `current` reads as
  // success (emerald), `available` as attention (warning), and the rest stay
  // muted. Kept colour-coded but quiet so the card never feels alarming.
  const { status }: { status: RenoDxFreshness } = $props();

  const LABEL = {
    current: 'gameDetails.renodx.fresh.current',
    available: 'gameDetails.renodx.fresh.available',
    unknown: 'gameDetails.renodx.fresh.unknown',
    untracked: 'gameDetails.renodx.updatesNotTracked',
    checking: 'gameDetails.renodx.fresh.checking',
  } satisfies Record<RenoDxFreshness, MessageKey>;

  const TINT = {
    current: 'border-transparent bg-emerald-500/10 text-emerald-600 dark:text-emerald-400',
    available: 'border-transparent bg-warning/10 text-warning',
    unknown: 'text-muted-foreground',
    untracked: 'text-muted-foreground',
    checking: 'text-muted-foreground',
  } satisfies Record<RenoDxFreshness, string>;
</script>

<Badge variant="outline" class={TINT[status]}>
  {#if status === 'current'}
    <CircleCheckIcon aria-hidden="true" />
  {:else if status === 'available'}
    <CircleArrowUpIcon aria-hidden="true" />
  {:else if status === 'checking'}
    <Spinner class="size-3" />
  {:else}
    <InfoIcon aria-hidden="true" />
  {/if}
  {t(LABEL[status])}
</Badge>
