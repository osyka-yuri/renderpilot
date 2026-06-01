<script lang="ts">
  import CopyIcon from '@lucide/svelte/icons/copy';
  import { Button, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import { t } from '@shared/i18n';
  import { toast } from 'svelte-sonner';
  import type { LibraryManifestEntry } from '@entities/library';

  type CopyStatus = 'idle' | 'copied' | 'failed';

  let { entry }: { entry: LibraryManifestEntry } = $props();

  const dllSha256Hash = $derived(entry.files.dll.hashes.sha256);

  let copyStatus = $state<CopyStatus>('idle');
  let resetTimer: ReturnType<typeof setTimeout> | undefined;

  const copyButtonLabel = $derived(
    copyStatus === 'copied'
      ? t('libraries.hash.copied')
      : copyStatus === 'failed'
        ? t('libraries.hash.failed')
        : t('libraries.hash.copy'),
  );
  const statusMessage = $derived(
    copyStatus === 'copied'
      ? t('libraries.hash.copied')
      : copyStatus === 'failed'
        ? t('libraries.hash.failed')
        : '',
  );

  $effect(() => {
    return () => {
      if (resetTimer !== undefined) {
        clearTimeout(resetTimer);
      }
    };
  });

  function scheduleReset(delayMs: number) {
    if (resetTimer !== undefined) {
      clearTimeout(resetTimer);
    }

    resetTimer = setTimeout(() => {
      copyStatus = 'idle';
      resetTimer = undefined;
    }, delayMs);
  }

  async function copyHashToClipboard() {
    try {
      await navigator.clipboard.writeText(dllSha256Hash);

      copyStatus = 'copied';
      toast.success(t('libraries.hash.copiedToast'));
      scheduleReset(2000);
    } catch (error) {
      console.error('Failed to copy DLL SHA-256 hash:', error);

      copyStatus = 'failed';
      toast.error(t('libraries.hash.failed'));
      scheduleReset(3000);
    }
  }
</script>

<div class="flex min-w-0 items-center gap-1">
  <code class="truncate rounded-sm bg-muted px-1 text-xs">
    {dllSha256Hash}
  </code>

  <Tooltip>
    <TooltipTrigger>
      <Button
        variant="ghost"
        size="icon"
        class="size-6"
        onclick={copyHashToClipboard}
        aria-label={copyButtonLabel}
      >
        <CopyIcon class="size-3" />
      </Button>
    </TooltipTrigger>
    <TooltipContent>{t('libraries.hash.copy')}</TooltipContent>
  </Tooltip>

  <span class="sr-only" aria-live="polite">
    {statusMessage}
  </span>
</div>
