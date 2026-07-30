<script lang="ts">
  import { formatBytes, formatPercent } from '@shared/format';
  import { getLocale, t } from '@shared/i18n';
  import { Progress, Spinner } from '@shared/ui';

  import { phaseStatusKey, type UpdateProgressPhase } from '../model/dialog-view';
  import type { DownloadProgressView } from '../model/types';

  type Props = {
    phase: UpdateProgressPhase;
    progress?: DownloadProgressView | null;
  };

  const { phase, progress = null }: Props = $props();

  const locale = $derived(getLocale());
  const showsProgressBar = $derived(phase === 'downloading' || phase === 'verifying');
  const isDownloadPhase = $derived(phase === 'downloading');
  const determinateRatio = $derived(showsProgressBar ? (progress?.ratio ?? null) : null);
  const isDeterminate = $derived(determinateRatio !== null);

  const statusLabel = $derived(t(phaseStatusKey(phase)));
  const percentLabel = $derived.by(() =>
    determinateRatio === null ? null : formatPercent(determinateRatio, locale),
  );

  const detailLabel = $derived.by(() => {
    if (phase === 'verifying') {
      return t('settings.about.updateDialog.verifyingDescription');
    }

    if (!isDownloadPhase || !progress) {
      return null;
    }

    if (progress.totalBytes !== null) {
      return t('settings.about.updateDialog.downloadingBytesTotal', {
        received: formatBytes(progress.receivedBytes, locale),
        total: formatBytes(progress.totalBytes, locale),
      });
    }

    if (progress.receivedBytes > 0) {
      return t('settings.about.updateDialog.downloadingBytes', {
        received: formatBytes(progress.receivedBytes, locale),
      });
    }

    return null;
  });

  const progressLabel = $derived(t('settings.about.updateDialog.progressAria'));
</script>

<div class="flex flex-col gap-2" role="status" aria-live="polite" aria-atomic="true">
  <div class="flex items-center gap-2 text-sm font-medium">
    {#if !isDeterminate}
      <Spinner class="size-4" />
    {/if}
    <span>{statusLabel}</span>
    {#if isDeterminate}
      <span class="ms-auto text-muted-foreground tabular-nums">
        {percentLabel}
      </span>
    {/if}
  </div>

  {#if showsProgressBar}
    <Progress
      value={determinateRatio}
      max={1}
      aria-label={progressLabel}
      aria-valuetext={percentLabel ?? undefined}
      class={isDeterminate ? undefined : 'animate-pulse'}
    />
  {/if}

  {#if detailLabel}
    <p class="text-xs text-muted-foreground">{detailLabel}</p>
  {/if}
</div>
