<script lang="ts">
  import DownloadIcon from '@lucide/svelte/icons/download';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import Loader2Icon from '@lucide/svelte/icons/loader-2';
  import { Button, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import { describeCommandError } from '@shared/api';
  import { toast } from 'svelte-sonner';
  import type { LibraryManifestEntry } from '@entities/library';
  import type { LibrariesPageModel } from '../model/create-libraries-page-model.svelte';

  type Props = {
    entry: LibraryManifestEntry;
    pendingEntryAction: LibrariesPageModel['pendingEntryAction'];
    isDownloaded: boolean;
    onDownload: (id: string) => Promise<void>;
    onDelete: (id: string) => Promise<void>;
  };

  let { entry, pendingEntryAction, isDownloaded, onDownload, onDelete }: Props = $props();

  const entryId = $derived(entry.entry_id);
  const isBusy = $derived(pendingEntryAction !== null && pendingEntryAction.entryId === entryId);

  const actionLabel = $derived(isDownloaded ? 'Delete' : 'Download');

  async function handleActionClick() {
    if (isBusy) return;

    try {
      if (isDownloaded) {
        await onDelete(entryId);
        toast.success(`Library ${entry.version.value} deleted`);
        return;
      }

      await onDownload(entryId);
      toast.success(`Library ${entry.version.value} downloaded`);
    } catch (error) {
      toast.error(`${actionLabel} failed: ${describeCommandError(error)}`);
    }
  }
</script>

<div class="flex items-center justify-end gap-2">
  <Tooltip>
    <TooltipTrigger>
      <Button
        variant="ghost"
        size="icon"
        disabled={isBusy}
        onclick={handleActionClick}
        aria-label={actionLabel}
      >
        {#if isBusy}
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
