<script lang="ts">
  import DownloadIcon from '@lucide/svelte/icons/download';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import Loader2Icon from '@lucide/svelte/icons/loader-2';
  import { publishErrorNotification, publishSuccessNotification } from '@shared/notifications';
  import { Button, DownloadProgressBar, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import { t } from '@shared/i18n';
  import {
    shouldDeleteLibraryPackage,
    type LibraryPackageRow,
  } from '../model/libraries-page-model';
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
  const shouldDelete = $derived(shouldDeleteLibraryPackage(row));

  const shortActionLabel = $derived(
    shouldDelete ? t('libraries.actions.delete') : t('libraries.actions.download'),
  );
  const actionLabel = $derived(
    shouldDelete
      ? t('libraries.actions.deleteVersion', { version: row.release.version })
      : t('libraries.actions.downloadVersion', { version: row.release.version }),
  );

  async function handleActionClick() {
    if (isActionDisabled) {
      return;
    }

    // The model returns `false` when it ignored the action (e.g. a catalog
    // load/refresh is running) — never report success for an action that
    // never ran.
    try {
      if (shouldDelete) {
        if (await onDelete(packageId)) {
          publishSuccessNotification(
            t('libraries.actions.deletedToast', { version: row.release.version }),
          );
        }
        return;
      }

      if (await onDownload(packageId)) {
        publishSuccessNotification(
          t('libraries.actions.downloadedToast', { version: row.release.version }),
        );
      }
    } catch {
      publishErrorNotification(t('libraries.actions.failedToast', { action: shortActionLabel }));
    }
  }
</script>

<div class="flex items-center justify-end gap-2">
  <DownloadProgressBar ids={[packageId]} active={isDownloading} />
  <Tooltip>
    <TooltipTrigger>
      {#snippet child({ props })}
        <Button
          {...props}
          variant="ghost"
          size="icon"
          disabled={isActionDisabled}
          onclick={handleActionClick}
          aria-label={actionLabel}
        >
          {#if pendingAction !== null}
            <Loader2Icon class="animate-spin" aria-hidden="true" />
          {:else if shouldDelete}
            <Trash2Icon aria-hidden="true" />
          {:else}
            <DownloadIcon aria-hidden="true" />
          {/if}
        </Button>
      {/snippet}
    </TooltipTrigger>
    <TooltipContent>
      {actionLabel}
    </TooltipContent>
  </Tooltip>
</div>
