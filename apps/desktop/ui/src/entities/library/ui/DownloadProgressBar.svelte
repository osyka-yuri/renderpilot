<script lang="ts">
  import { Progress, Spinner } from '@shared/ui';
  import { t, translateKey } from '@shared/i18n';
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

  // A phase may be a raw label ("RenoDX add-on …") or an i18n key
  // ("renodx.phase.finalizing"). `translateKey` falls back to the raw text when
  // the key is not in the catalog, so existing raw labels render unchanged.
  const phaseLabel = $derived(progress?.phase ? translateKey(progress.phase, progress.phase) : '');
</script>

{#if progress && progress.total > 0}
  <div class={cn('flex items-center gap-2', className)}>
    {#if phaseLabel}
      <span class="text-xs text-muted-foreground whitespace-nowrap truncate max-w-[120px]"
        >{phaseLabel}</span
      >
    {/if}
    <div class="w-16">
      <Progress
        value={progress.downloaded}
        max={progress.total}
        aria-label={t('common.downloadProgress')}
      />
    </div>
  </div>
{:else if progress?.total === 0 && phaseLabel}
  <!-- Indeterminate phase (e.g. disk I/O / record persistence after a download
  finishes): no byte total to fill a bar against, so show a spinner + label. -->
  <div class={cn('flex items-center gap-2', className)}>
    <Spinner class="size-4" />
    <span class="text-xs text-muted-foreground whitespace-nowrap truncate max-w-[160px]"
      >{phaseLabel}</span
    >
  </div>
{/if}
