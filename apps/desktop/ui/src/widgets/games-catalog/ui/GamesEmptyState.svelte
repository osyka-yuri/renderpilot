<script lang="ts">
  import { cn, type VoidHandler } from '@shared/utils';
  import type { HTMLAttributes } from 'svelte/elements';
  import { Button, EmptyStatePanel } from '@shared/ui';

  const noop: VoidHandler = (): void => {
    // Intentionally empty.
  };

  type Props = HTMLAttributes<HTMLElement> & {
    busy?: boolean;
    scanButtonLabel?: string;
    onRefresh?: VoidHandler;
    onScan?: VoidHandler;
  };

  const {
    busy = false,
    scanButtonLabel = 'Scan Folder',
    onRefresh = noop,
    onScan = noop,
    class: className = '',
    ...rest
  }: Props = $props();
</script>

<EmptyStatePanel {...rest} class={cn('grid justify-items-start gap-3 p-6', className)}>
  <div
    class={cn(
      'grid size-10 place-items-center rounded-2xl bg-accent-soft font-bold',
      'tracking-wider text-accent-strong',
    )}
    aria-hidden="true"
  >
    RP
  </div>

  <div class="grid max-w-xl gap-1">
    <h3 class="text-base/tight font-semibold">No scanned games yet</h3>
    <p>
      Select a game folder to populate the dashboard with components, updates, backup state, and
      quick actions.
    </p>
  </div>

  <div
    class={cn('flex flex-wrap gap-2', 'max-sm:w-full max-sm:flex-col-reverse max-sm:items-stretch')}
  >
    <Button variant="secondary" size="sm" disabled={busy} loading={busy} onclick={onRefresh}>
      Refresh Libraries
    </Button>

    <Button variant="primary" size="sm" disabled={busy} loading={busy} onclick={onScan}>
      {scanButtonLabel}
    </Button>
  </div>
</EmptyStatePanel>
