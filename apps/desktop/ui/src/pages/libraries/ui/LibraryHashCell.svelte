<script lang="ts">
  import CopyIcon from '@lucide/svelte/icons/copy';
  import { Button, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import { t } from '@shared/i18n';
  import { reportClientError } from '@shared/errors';
  import { publishErrorNotification, publishSuccessNotification } from '@shared/notifications';
  import type { LibraryPackageRow } from '../model/libraries-page-model';

  let { row }: { row: LibraryPackageRow } = $props();

  const dllSha256Hash = $derived(row.primary_sha256);
  const copyButtonLabel = $derived(
    t('libraries.hash.copyVersion', { version: row.release.version }),
  );

  async function copyHashToClipboard() {
    try {
      await navigator.clipboard.writeText(dllSha256Hash);

      publishSuccessNotification(t('libraries.hash.copiedToast'));
    } catch (error) {
      reportClientError('copy_library_hash', error);

      publishErrorNotification(t('libraries.hash.failed'));
    }
  }
</script>

<div class="flex min-w-0 items-center gap-1">
  <code class="truncate rounded-sm bg-muted px-1 text-xs">
    {dllSha256Hash}
  </code>

  <Tooltip>
    <TooltipTrigger>
      {#snippet child({ props })}
        <Button
          {...props}
          variant="ghost"
          size="icon"
          class="size-6"
          onclick={copyHashToClipboard}
          aria-label={copyButtonLabel}
        >
          <CopyIcon class="size-3" aria-hidden="true" />
        </Button>
      {/snippet}
    </TooltipTrigger>
    <TooltipContent>{t('libraries.hash.copy')}</TooltipContent>
  </Tooltip>
</div>
