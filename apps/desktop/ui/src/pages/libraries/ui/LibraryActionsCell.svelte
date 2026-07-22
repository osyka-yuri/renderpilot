<script lang="ts">
  import DownloadIcon from '@lucide/svelte/icons/download';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import Loader2Icon from '@lucide/svelte/icons/loader-2';
  import { Button, DownloadProgressBar, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import { describeCommandError } from '@shared/api';
  import { t } from '@shared/i18n';
  import { toast } from 'svelte-sonner';
  import type { LibraryPackageRow } from '../model/libraries-page-model';
  import type { LibrariesPageModel } from '../model/create-libraries-page-model.svelte';

  type Props = {
    row: LibraryPackageRow;
    pendingActions: LibrariesPageModel['pendingActions'];
    actionsDisabled: () => boolean;
    onDownload: (id: string) => Promise<boolean>;
    onDelete: (id: string) => Promise<boolean>;
  };

  let { row, pendingActions, actionsDisabled, onDownload, onDelete }: Props = $props();

  const packageId = $derived(row.package_id);
  const pendingAction = $derived(pendingActions.get(packageId) ?? null);
  const isActionDisabled = $derived(pendingAction !== null || actionsDisabled());
  const isDownloading = $derived(pendingAction === 'download');
  const isDownloaded = $derived(row.is_downloaded);

  const actionLabel = $derived(
    isDownloaded ? t('libraries.actions.delete') : t('libraries.actions.download'),
  );

  async function handleActionClick() {
    if (isActionDisabled) {
      return;
    }

    // The model returns `false` when it ignored the action (e.g. a catalog
    // load/refresh is running) — never report success for an action that
    // never ran.
    try {
      if (isDownloaded) {
        if (await onDelete(packageId)) {
          toast.success(t('libraries.actions.deletedToast', { version: row.release.version }));
        }
        return;
      }

      if (await onDownload(packageId)) {
        toast.success(t('libraries.actions.downloadedToast', { version: row.release.version }));
      }
    } catch (error) {
      toast.error(
        t('libraries.actions.failedToast', {
          action: actionLabel,
          error: describeCommandError(error),
        }),
      );
    }
  }
</script>

<div class="flex items-center justify-center gap-2">
  <DownloadProgressBar ids={[packageId]} active={isDownloading} />
  <Tooltip>
    <TooltipTrigger>
      <Button
        variant="ghost"
        size="icon"
        disabled={isActionDisabled}
        onclick={handleActionClick}
        aria-label={actionLabel}
      >
        {#if pendingAction !== null}
          <Loader2Icon class="animate-spin" aria-hidden="true" />
        {:else if isDownloaded}
          <Trash2Icon aria-hidden="true" />
        {:else}
          <DownloadIcon aria-hidden="true" />
        {/if}
      </Button>
    </TooltipTrigger>
    <TooltipContent>
      {actionLabel}
    </TooltipContent>
  </Tooltip>
</div>
