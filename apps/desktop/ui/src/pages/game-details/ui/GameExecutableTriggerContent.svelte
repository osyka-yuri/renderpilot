<script lang="ts">
  import AppWindowIcon from '@lucide/svelte/icons/app-window';
  import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
  import LockKeyholeIcon from '@lucide/svelte/icons/lock-keyhole';
  import { t } from '@shared/i18n';
  import { Badge } from '@shared/ui';

  type Props = {
    label: string;
    isOverride: boolean;
    locked: boolean;
  };

  const { label, isOverride, locked }: Props = $props();
</script>

<AppWindowIcon class="size-4 opacity-70" aria-hidden="true" />
<span class="max-w-40 truncate">{label}</span>
{#if isOverride}
  <Badge variant="secondary" class="px-1.5 py-0 text-[10px]">
    {t('gameDetails.executable.customBadge')}
  </Badge>
{/if}
{#if locked}
  <LockKeyholeIcon class="size-3.5 opacity-60" aria-hidden="true" />
{:else}
  <ChevronDownIcon class="size-3.5 opacity-50" aria-hidden="true" />
{/if}
