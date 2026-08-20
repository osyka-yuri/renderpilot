<script lang="ts">
  import { Dialog, DialogClose, DialogContent, DialogTitle } from './dialog';
  import { Select, SelectContent, SelectItem, SelectTrigger } from './select';

  let selectOpen = $state(false);
  let dialogOpen = $state(false);

  export function setSelectOpen(open: boolean): void {
    selectOpen = open;
  }

  export function setDialogOpen(open: boolean): void {
    dialogOpen = open;
  }
</script>

<Select type="single" bind:open={selectOpen}>
  <SelectTrigger>Choose version</SelectTrigger>
  <SelectContent preventScroll={true}>
    <SelectItem value="preview">Preview</SelectItem>
  </SelectContent>
</Select>

<Dialog bind:open={dialogOpen}>
  <DialogContent closeLabel="Close test dialog">
    <DialogTitle>Confirm change</DialogTitle>
    <DialogClose>Cancel</DialogClose>
  </DialogContent>
</Dialog>
