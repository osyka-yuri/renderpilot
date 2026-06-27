<script lang="ts">
  import ArrowUpToLineIcon from '@lucide/svelte/icons/arrow-up-to-line';
  import Loader2Icon from '@lucide/svelte/icons/loader-2';
  import { Button, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import { BatchDownloadProgressBar } from '@entities/library';
  import { describeCommandError } from '@shared/api';
  import { t } from '@shared/i18n';
  import { toast } from 'svelte-sonner';
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
        toast.info(t('libraries.actions.downloadAllNoneToast'));
        return;
      }

      if (failed > 0) {
        toast.error(t('libraries.actions.downloadAllPartialToast', { succeeded, failed }));
        return;
      }

      toast.success(t('libraries.actions.downloadAllDoneToast', { count: succeeded }));
    } catch (error) {
      toast.error(
        t('libraries.actions.failedToast', {
          action: t('libraries.actions.downloadAll'),
          error: describeCommandError(error),
        }),
      );
    }
  }
</script>

<div class="flex items-center justify-end gap-2">
  <BatchDownloadProgressBar
    value={model.bulkProgressValue}
    max={model.bulkTotal}
    active={model.bulkDownloading}
    ariaLabel={label}
  />
  <Tooltip>
    <TooltipTrigger>
      <Button
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
    </TooltipTrigger>
    <TooltipContent>{tooltip}</TooltipContent>
  </Tooltip>
</div>
