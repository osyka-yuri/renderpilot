<script lang="ts">
  import type { LibraryManifestEntry } from '@entities/library';
  import { formatVersionLabel } from '../model/libraries-page-model';

  let { entry, showFileName }: { entry: LibraryManifestEntry; showFileName: boolean } = $props();

  const versionLabel = $derived(formatVersionLabel(entry));
</script>

<div class="flex flex-col gap-0.5">
  <span>{versionLabel}</span>
  {#if showFileName}
    <span class="truncate text-xs text-muted-foreground" title={entry.library.file_name}>
      {entry.library.file_name}
    </span>
  {/if}
</div>
