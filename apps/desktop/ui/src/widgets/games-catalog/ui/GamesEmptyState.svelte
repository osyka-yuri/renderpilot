<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements';
  import { type VoidHandler } from '@shared/utils';
  import { Button } from '@shared/ui';

  const noop: VoidHandler = (): void => {
    // Intentionally empty.
  };

  type Props = HTMLAttributes<HTMLElement> & {
    busy?: boolean;
    scanButtonLabel?: string;
    onRefresh?: VoidHandler;
    onScan?: VoidHandler;
  };

  let {
    busy = false,
    scanButtonLabel = 'Scan Folder',
    onRefresh = noop,
    onScan = noop,
    class: className = '',
    ...rest
  }: Props = $props();
</script>

<div {...rest} class={['games-empty-state', className]}>
  <div class="empty-icon" aria-hidden="true">RP</div>

  <div class="empty-copy">
    <h3 class="empty-title">No scanned games yet</h3>
    <p class="empty-description">
      Select a game folder to populate the dashboard with components, updates, backup state, and
      quick actions.
    </p>
  </div>

  <div class="action-group">
    <Button variant="secondary" size="sm" disabled={busy} loading={busy} onclick={onRefresh}>
      Refresh Libraries
    </Button>

    <Button variant="primary" size="sm" disabled={busy} loading={busy} onclick={onScan}>
      {scanButtonLabel}
    </Button>
  </div>
</div>

<style>
  .games-empty-state {
    display: grid;
    justify-items: start;
    gap: var(--space-3);
    padding: var(--space-6);
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-xl);
    background: color-mix(in srgb, var(--bg-card) 62%, transparent);
  }

  .empty-icon {
    display: grid;
    width: 2.5rem;
    height: 2.5rem;
    place-items: center;
    border-radius: var(--radius-lg);
    background: var(--accent-soft);
    color: var(--accent-strong);
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  .empty-copy {
    display: grid;
    gap: var(--space-1);
    max-width: 36rem;
  }

  .empty-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
    line-height: 1.2;
  }

  .empty-description {
    margin: 0;
  }

  .action-group {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  @media (max-width: 560px) {
    .action-group {
      width: 100%;
      flex-direction: column-reverse;
      align-items: stretch;
    }
  }
</style>
