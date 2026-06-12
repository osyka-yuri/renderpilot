<script lang="ts">
  import { Progress } from '@shared/ui';
  import { t } from '@shared/i18n';
  import { cn } from '@shared/classnames';
  import { latestDownloadProgress } from '../model/download-progress.svelte';

  type Props = {
    /** The artifact / entry ids this bar tracks. */
    ids: readonly string[];
    /** Whether the owning control is in a busy/downloading state. */
    active: boolean;
    class?: string;
  };

  const { ids, active, class: className }: Props = $props();

  const progress = $derived(active && ids.length > 0 ? latestDownloadProgress(ids) : null);
</script>

{#if progress && progress.total > 0}
  <div class={cn('w-16', className)}>
    <Progress
      value={progress.downloaded}
      max={progress.total}
      aria-label={t('common.downloadProgress')}
    />
  </div>
{/if}
