<script lang="ts">
  import type { D3d12ExecutableMutationAction } from '@shared/model';

  import D3d12ExecutableConfirmDialog from './D3d12ExecutableConfirmDialog.svelte';

  type Props = {
    busy: boolean;
    actions: D3d12ExecutableMutationAction[];
    reason?: 'swap' | 'update_all';
    onConfirm: () => void;
  };

  const { busy, actions, reason = 'swap', onConfirm }: Props = $props();
  let open = $state(true);

  export function close(): void {
    open = false;
  }

  function confirm(): void {
    open = false;
    onConfirm();
  }
</script>

<D3d12ExecutableConfirmDialog
  {open}
  {busy}
  {actions}
  {reason}
  onOpenChange={(next: boolean) => {
    open = next;
  }}
  onConfirm={confirm}
/>
