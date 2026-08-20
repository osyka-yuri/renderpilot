<script lang="ts">
  import { TooltipProvider } from '@shared/ui';

  import type { LibraryPackageRow } from '../model/libraries-page-model';
  import LibraryHashCell from './LibraryHashCell.svelte';
  import LibraryLegalCell from './LibraryLegalCell.svelte';

  type Props = {
    row: LibraryPackageRow;
    onOpen: (row: LibraryPackageRow) => void;
    cell?: 'hash' | 'legal';
  };

  const { row, onOpen, cell = 'legal' }: Props = $props();
</script>

<TooltipProvider>
  {#if cell === 'hash'}
    <LibraryHashCell {row} />
  {:else}
    <LibraryLegalCell {row} {onOpen} />
  {/if}
</TooltipProvider>
