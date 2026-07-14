<script lang="ts">
  import { t } from '@shared/i18n';
  import { formatBytes } from '@shared/format';
  import { Progress, Spinner } from '@shared/ui';

  import { phaseStatusKey, type UpdateProgressPhase } from '../model/dialog-view';
  import type { DownloadProgressView } from '../model/types';

  type Props = {
    phase: UpdateProgressPhase;
    progress?: DownloadProgressView | null;
  };

  const { phase, progress = null }: Props = $props();

  const showsProgressBar = $derived(phase === 'downloading' || phase === 'verifying');
  const isDownloadPhase = $derived(phase === 'downloading');
  const isDeterminate = $derived(
    showsProgressBar && progress !== null && progress.percent !== null,
  );

  const statusLabel = $derived(t(phaseStatusKey(phase)));

  const detailLabel = $derived.by(() => {
    if (phase === 'verifying') {
      return t('settings.about.updateDialog.verifyingDescription');
    }

    if (!isDownloadPhase || !progress) {
      return null;
    }

    if (progress.totalBytes !== null) {
      return t('settings.about.updateDialog.downloadingBytesTotal', {
        received: formatBytes(progress.receivedBytes),
        total: formatBytes(progress.totalBytes),
      });
    }

    if (progress.receivedBytes > 0) {
      return t('settings.about.updateDialog.downloadingBytes', {
        received: formatBytes(progress.receivedBytes),
      });
    }

    return null;
  });

  const progressAria = $derived(
    isDeterminate
      ? t('settings.about.updateDialog.progressAria', {
          percent: Math.round(progress?.percent ?? 0),
        })
      : t('settings.about.updateDialog.indeterminateProgressAria'),
  );
</script>

<div class="flex flex-col gap-2" role="status" aria-live="polite" aria-atomic="true">
  <div class="flex items-center gap-2 text-sm font-medium">
    {#if !isDeterminate}
      <Spinner class="size-4" />
    {/if}
    <span>{statusLabel}</span>
    {#if isDeterminate && progress}
      <span class="ms-auto tabular-nums text-muted-foreground">
        {Math.round(progress.percent ?? 0)}%
      </span>
    {/if}
  </div>

  {#if showsProgressBar}
    <Progress
      value={isDeterminate ? (progress?.percent ?? 0) : undefined}
      max={100}
      aria-label={progressAria}
      aria-valuemin={isDeterminate ? 0 : undefined}
      aria-valuemax={isDeterminate ? 100 : undefined}
      aria-valuenow={isDeterminate ? Math.round(progress?.percent ?? 0) : undefined}
      class={isDeterminate ? undefined : 'animate-pulse'}
    />
  {/if}

  {#if detailLabel}
    <p class="text-xs text-muted-foreground">{detailLabel}</p>
  {/if}
</div>
