<script lang="ts">
  import { t, translateExternalMessage } from '@shared/i18n';
  import { cn } from '@shared/classnames';
  import { latestDownloadProgress } from '@shared/lib';

  import { Progress } from '../progress';
  import { Spinner } from '../spinner';

  type Props = {
    /** The artifact / entry ids this bar tracks. */
    ids: readonly string[];
    /** Whether the owning control is in a busy/downloading state. */
    active: boolean;
    class?: string;
  };

  const { ids, active, class: className }: Props = $props();

  const progress = $derived(active && ids.length > 0 ? latestDownloadProgress(ids) : null);

  // Indeterminate finalization keeps its phase text beside the spinner; byte-tracked
  // downloads intentionally render only the compact progress bar.
  const phaseLabel = $derived(
    progress?.phase
      ? translateExternalMessage({ key: progress.phase, fallback: progress.phase })
      : '',
  );
</script>

{#if progress && progress.total > 0}
  <div class={cn('w-16', className)}>
    <Progress
      value={progress.downloaded}
      max={progress.total}
      aria-label={t('common.downloadProgress')}
    />
  </div>
{:else if progress?.total === 0 && phaseLabel}
  <!-- Disk I/O / record persistence after a download: no byte total means a spinner. -->
  <div class={cn('flex items-center gap-2', className)}>
    <Spinner class="size-4" />
    <span class="max-w-[160px] truncate text-xs whitespace-nowrap text-muted-foreground"
      >{phaseLabel}</span
    >
  </div>
{/if}
