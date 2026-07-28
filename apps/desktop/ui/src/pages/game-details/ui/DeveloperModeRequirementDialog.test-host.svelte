<script lang="ts">
  import type { DeveloperModePlanBlocker } from '../model/d3d12-preflight';
  import DeveloperModeRequirementDialog from './DeveloperModeRequirementDialog.svelte';

  type Props = {
    onRetry: () => void;
    onCancel: () => void;
  };

  const { onRetry, onCancel }: Props = $props();

  let open = $state(true);
  let blocker = $state<DeveloperModePlanBlocker>('developer_mode_required');
  let retrying = $state(false);
  let stillDisabledAfterRetry = $state(false);

  export function close(): void {
    open = false;
  }

  export function show(): void {
    open = true;
  }

  export function showUnavailable(): void {
    blocker = 'developer_mode_check_unavailable';
    retrying = false;
    stillDisabledAfterRetry = false;
  }

  export function completeRetryAsDisabled(): void {
    blocker = 'developer_mode_required';
    retrying = false;
    stillDisabledAfterRetry = true;
  }

  function setOpen(next: boolean): void {
    open = next;
    if (!next) {
      onCancel();
    }
  }

  function retry(): void {
    retrying = true;
    onRetry();
  }
</script>

<DeveloperModeRequirementDialog
  {open}
  {blocker}
  {retrying}
  {stillDisabledAfterRetry}
  onOpenChange={setOpen}
  onRetry={retry}
/>
