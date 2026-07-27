<script lang="ts">
  import type { LibraryPackageRow } from '../model/libraries-page-model';

  import LibraryLegalDocumentsSheet from './LibraryLegalDocumentsSheet.svelte';

  type Props = {
    initialRow: LibraryPackageRow;
    onClose: () => void;
  };

  const { initialRow, onClose }: Props = $props();
  let closed = $state(false);
  const row = $derived(closed ? null : initialRow);

  export function close(): void {
    closed = true;
  }

  function handleClose(): void {
    close();
    onClose();
  }
</script>

<LibraryLegalDocumentsSheet {row} onClose={handleClose} />
