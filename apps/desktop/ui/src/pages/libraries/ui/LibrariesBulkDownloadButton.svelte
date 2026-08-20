<script lang="ts">
  import ArrowUpToLineIcon from '@lucide/svelte/icons/arrow-up-to-line';
  import Loader2Icon from '@lucide/svelte/icons/loader-2';
  import {
    publishErrorNotification,
    publishInfoNotification,
    publishSuccessNotification,
  } from '@shared/notifications';
  import { Button, Progress, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import { t } from '@shared/i18n';
  import type { LibrariesPageModel } from '../model/create-libraries-page-model.svelte';

  type Props = {
    model: LibrariesPageModel;
  };

  let { model }: Props = $props();

  const pendingCount = $derived(model.latestStablePendingCount);
  const hasPending = $derived(pendingCount > 0);
  const disabled = $derived(model.isBusy || !hasPending);
  const label = $derived(
    hasPending
      ? t('libraries.actions.downloadAllCount', { count: pendingCount })
      : t('libraries.actions.downloadAll'),
  );
  const tooltip = $derived(
    hasPending
      ? t('libraries.actions.downloadAllTooltip', { count: pendingCount })
      : t('libraries.actions.downloadAllUpToDate'),
  );

  async function handleClick() {
    if (model.isBusy) {
      return;
    }

    try {
      const { succeeded, failed } = await model.downloadAllLatest();

      if (succeeded === 0 && failed === 0) {
        publishInfoNotification(t('libraries.actions.downloadAllNoneToast'));
        return;
      }

      if (failed > 0) {
        publishErrorNotification(
          t('libraries.actions.downloadAllPartialToast', { succeeded, failed }),
        );
        return;
      }

      publishSuccessNotification(t('libraries.actions.downloadAllDoneToast', { count: succeeded }));
    } catch {
      publishErrorNotification(
        t('libraries.actions.failedToast', { action: t('libraries.actions.downloadAll') }),
      );
    }
  }
</script>

<div class="flex items-center justify-end gap-2">
  {#if model.bulkDownloading && model.bulkTotal > 0}
    <div class="w-16">
      <Progress value={model.bulkProgressValue} max={model.bulkTotal} aria-label={label} />
    </div>
  {/if}
  <Tooltip>
    <TooltipTrigger>
      {#snippet child({ props })}
        <Button
          {...props}
          variant="default"
          size="sm"
          {disabled}
          aria-busy={model.bulkDownloading}
          onclick={handleClick}
        >
          {#if model.bulkDownloading}
            <Loader2Icon class="animate-spin" aria-hidden="true" />
          {:else}
            <ArrowUpToLineIcon aria-hidden="true" />
          {/if}
          {label}
        </Button>
      {/snippet}
    </TooltipTrigger>
    <TooltipContent>{tooltip}</TooltipContent>
  </Tooltip>
</div>
