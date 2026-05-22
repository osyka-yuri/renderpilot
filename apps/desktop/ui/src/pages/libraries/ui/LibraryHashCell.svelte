<script lang="ts">
  import CopyIcon from '@lucide/svelte/icons/copy';
  import { Button, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import { toast } from 'svelte-sonner';
  import type { LibraryManifestEntry } from '@entities/library';

  type CopyStatus = 'idle' | 'copied' | 'failed';

  const STATUS_CONFIG: Record<CopyStatus, { label: string; announcement: string }> = {
    idle: { label: 'Copy hash', announcement: '' },
    copied: { label: 'Hash copied', announcement: 'Hash copied' },
    failed: { label: 'Failed to copy hash', announcement: 'Failed to copy hash' },
  };

  let { entry }: { entry: LibraryManifestEntry } = $props();

  const dllSha256Hash = $derived(entry.files.dll.hashes.sha256);

  let copyStatus = $state<CopyStatus>('idle');
  let resetTimer: ReturnType<typeof setTimeout> | undefined;

  const copyButtonLabel = $derived(STATUS_CONFIG[copyStatus].label);
  const statusMessage = $derived(STATUS_CONFIG[copyStatus].announcement);

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
      toast.success('Hash copied to clipboard');
      scheduleReset(2000);
    } catch (error) {
      console.error('Failed to copy DLL SHA-256 hash:', error);

      copyStatus = 'failed';
      toast.error('Failed to copy hash');
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
    <TooltipContent>Copy hash</TooltipContent>
  </Tooltip>

  <span class="sr-only" aria-live="polite">
    {statusMessage}
  </span>
</div>
