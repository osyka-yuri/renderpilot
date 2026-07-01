<script lang="ts">
  import CircleArrowUpIcon from '@lucide/svelte/icons/circle-arrow-up';
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import InfoIcon from '@lucide/svelte/icons/info';
  import { t } from '@shared/i18n';
  import type { MessageKey } from '@shared/i18n';
  import { Badge, Spinner } from '@shared/ui';

  import type { RenoDxFreshness, UpdateStatus } from '../model/types';

  type RenoDxBadgeStatus = RenoDxFreshness | UpdateStatus;

  type Props = {
    status: RenoDxBadgeStatus;
  };

  type StatusIcon = 'success' | 'update' | 'checking' | 'info';

  type StatusMeta = {
    label: MessageKey;
    tint: string;
    icon: StatusIcon;
  };

  const MUTED_TINT = 'text-muted-foreground';
  const SUCCESS_TINT =
    'border-transparent bg-emerald-500/10 text-emerald-600 dark:text-emerald-400';
  const WARNING_TINT = 'border-transparent bg-warning/10 text-warning';

  const STATUS_META = {
    current: {
      label: 'gameDetails.renodx.fresh.current',
      tint: SUCCESS_TINT,
      icon: 'success',
    },
    available: {
      label: 'gameDetails.renodx.fresh.available',
      tint: WARNING_TINT,
      icon: 'update',
    },
    unknown: {
      label: 'gameDetails.renodx.fresh.unknown',
      tint: MUTED_TINT,
      icon: 'info',
    },
    untracked: {
      label: 'gameDetails.renodx.updatesNotTracked',
      tint: MUTED_TINT,
      icon: 'info',
    },
    checking: {
      label: 'gameDetails.renodx.fresh.checking',
      tint: MUTED_TINT,
      icon: 'checking',
    },
    channel_mismatch: {
      label: 'gameDetails.renodx.fresh.channelMismatch',
      tint: WARNING_TINT,
      icon: 'update',
    },
    unknown_needs_validation: {
      label: 'gameDetails.renodx.fresh.validationRequired',
      tint: MUTED_TINT,
      icon: 'info',
    },
  } satisfies Record<RenoDxBadgeStatus, StatusMeta>;

  const { status }: Props = $props();

  const meta = $derived(STATUS_META[status]);
</script>

<Badge variant="outline" class={meta.tint}>
  {#if meta.icon === 'success'}
    <CircleCheckIcon aria-hidden="true" />
  {:else if meta.icon === 'update'}
    <CircleArrowUpIcon aria-hidden="true" />
  {:else if meta.icon === 'checking'}
    <Spinner class="size-3" />
  {:else}
    <InfoIcon aria-hidden="true" />
  {/if}

  {t(meta.label)}
</Badge>
