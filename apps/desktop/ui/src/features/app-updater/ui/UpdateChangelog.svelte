<script lang="ts">
  import { t } from '@shared/i18n';
  import type { ReleaseNotesDocument } from '../model/release-notes';
  import ReleaseNotesInline from './ReleaseNotesInline.svelte';

  type Props = {
    document: ReleaseNotesDocument;
  };

  const { document }: Props = $props();

  const hasBlocks = $derived(document.blocks.length > 0);
</script>

{#if !hasBlocks}
  <p class="text-sm text-muted-foreground">
    {t('settings.about.updateDialog.noNotes')}
  </p>
{:else}
  <div class="flex flex-col gap-3 text-sm">
    {#each document.blocks as block, blockIndex (blockIndex)}
      {#if block.type === 'heading'}
        {#if block.level === 2}
          <h3 class="text-base font-semibold tracking-tight">
            <ReleaseNotesInline segments={block.content} />
          </h3>
        {:else}
          <h4 class="text-sm font-semibold tracking-tight">
            <ReleaseNotesInline segments={block.content} />
          </h4>
        {/if}
      {:else if block.type === 'paragraph'}
        <p class="text-sm/relaxed text-muted-foreground">
          <ReleaseNotesInline segments={block.content} />
        </p>
      {:else if block.type === 'list'}
        <ul class="list-disc space-y-1 ps-5 text-sm/relaxed text-muted-foreground">
          {#each block.items as item, itemIndex (itemIndex)}
            <li>
              <ReleaseNotesInline segments={item} />
            </li>
          {/each}
        </ul>
      {/if}
    {/each}

    {#if document.truncated}
      <p class="text-xs text-muted-foreground italic">
        {t('settings.about.updateDialog.notesTruncated')}
      </p>
    {/if}
  </div>
{/if}
