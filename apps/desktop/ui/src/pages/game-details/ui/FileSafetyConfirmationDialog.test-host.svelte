<script lang="ts">
  import { untrack } from 'svelte';

  import type { FileSafetyNotice } from '../model/file-safety-notice-policy';
  import FileSafetyConfirmationDialog from './FileSafetyConfirmationDialog.svelte';

  type Props = {
    initialNotice: FileSafetyNotice | null;
    onCancel: () => void;
    onConfirm: (rememberGeneralWarning: boolean) => void;
  };

  const { initialNotice, onCancel, onConfirm }: Props = $props();

  let notice = $state<FileSafetyNotice | null>(untrack(() => initialNotice));

  export function setNotice(next: FileSafetyNotice | null): void {
    notice = next;
  }
</script>

<FileSafetyConfirmationDialog {notice} actions={[]} {onCancel} {onConfirm} />
