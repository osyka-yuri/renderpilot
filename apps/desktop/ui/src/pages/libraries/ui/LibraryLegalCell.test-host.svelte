<script lang="ts">
  import { TooltipProvider } from '@shared/ui';

  import type { LibraryPackageRow } from '../model/libraries-page-model';
  import LibraryLegalCell from './LibraryLegalCell.svelte';

  type Props = {
    row: LibraryPackageRow;
    onOpen: (row: LibraryPackageRow) => void;
  };

  const { row, onOpen }: Props = $props();
</script>

<TooltipProvider>
  <LibraryLegalCell {row} {onOpen} />
</TooltipProvider>
