<script lang="ts">
  import PanelRightOpenIcon from '@lucide/svelte/icons/panel-right-open';
  import { t } from '@shared/i18n';
  import { buttonVariants, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';

  import type { LibraryPackageRow } from '../model/libraries-page-model';

  type Props = {
    row: LibraryPackageRow;
    onOpen: (row: LibraryPackageRow) => void;
  };

  const { row, onOpen }: Props = $props();

  const openLabel = $derived(
    t('libraries.documents.openForVersion', {
      name: row.display_name,
      version: row.release.version,
    }),
  );
</script>

{#if row.legal_documents.length === 0}
  <span class="text-muted-foreground">—</span>
{:else}
  <Tooltip>
    <TooltipTrigger
      type="button"
      class={buttonVariants({ variant: 'ghost', size: 'icon-sm' })}
      onclick={() => {
        onOpen(row);
      }}
      aria-label={openLabel}
    >
      <PanelRightOpenIcon class="size-4" aria-hidden="true" />
    </TooltipTrigger>
    <TooltipContent>{openLabel}</TooltipContent>
  </Tooltip>
{/if}
