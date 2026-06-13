<script lang="ts">
  import DownloadIcon from '@lucide/svelte/icons/download';
  import Loader2Icon from '@lucide/svelte/icons/loader-2';
  import { Button, Progress } from '@shared/ui';
  import { describeCommandError } from '@shared/api';
  import { t } from '@shared/i18n';
  import { toast } from 'svelte-sonner';
  import type { LibrariesPageModel } from '../model/create-libraries-page-model.svelte';

  type Props = {
    model: LibrariesPageModel;
  };

  let { model }: Props = $props();

  const pendingCount = $derived(model.latestStablePendingCount);
  const disabled = $derived(model.isBusy || pendingCount === 0);
  const label = $derived(computeLabel());

  function computeLabel(): string {
    if (model.bulkDownloading) {
      return t('libraries.actions.downloadAllInProgress', {
        done: model.bulkCompleted,
        total: model.bulkTotal,
      });
    }

    if (pendingCount > 0) {
      return t('libraries.actions.downloadAllCount', { count: pendingCount });
    }

    return t('libraries.actions.downloadAll');
  }

  async function handleClick() {
    if (model.isBusy) return;

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
  {#if model.bulkDownloading}
    <Progress
      value={model.bulkProgressValue}
      max={model.bulkTotal}
      class="w-16"
      aria-label={label}
    />
  {/if}
  <Button variant="outline" {disabled} aria-busy={model.bulkDownloading} onclick={handleClick}>
    {#if model.bulkDownloading}
      <Loader2Icon class="animate-spin" aria-hidden="true" />
    {:else}
      <DownloadIcon aria-hidden="true" />
    {/if}
    {label}
  </Button>
</div>
