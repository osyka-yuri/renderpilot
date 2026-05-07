<script lang="ts">
  import type { OperationSummary } from '@shared/api/types';
  import { formatLabel, formatTimestamp, statusTone } from '@shared/utils/presenters';
  import Badge from '@shared/ui/Badge.svelte';
  import Button from '@shared/ui/Button.svelte';

  export let operations: OperationSummary[] = [];
  export let busy = false;
  export let canRollback: (status: string) => boolean;
  export let onRollback: (operationId: string) => void;
</script>

<section class="content-section">
  <div class="section-head">
    <div>
      <p class="eyebrow">History</p>
      <h3>History</h3>
    </div>
    <Badge surface="outline" tone="muted">{operations.length} entries</Badge>
  </div>

  {#if operations.length === 0}
    <div class="empty-inline">No operations have been recorded for this game yet.</div>
  {:else}
    <div class="operation-list">
      {#each operations as operation}
        <article class="operation-card">
          <div class="operation-top">
            <div>
              <strong>{formatLabel(operation.kind)}</strong>
              <p>{formatTimestamp(operation.created_at)}</p>
            </div>
            <Badge pill tone={statusTone(operation.status)}>{formatLabel(operation.status)}</Badge>
          </div>

          <div class="operation-metrics three-up">
            <div>
              <span>Completed</span>
              <strong>{formatTimestamp(operation.completed_at)}</strong>
            </div>
            <div>
              <span>Backup status</span>
              <strong>{formatLabel(operation.backup_status)}</strong>
            </div>
            <div>
              <span>Items</span>
              <strong>{operation.item_count}</strong>
            </div>
          </div>

          <Button
            variant="secondary"
            size="sm"
            disabled={!canRollback(operation.status) || busy}
            loading={busy}
            onclick={() => onRollback(operation.operation_id)}
          >
            {busy ? 'Working...' : 'Rollback This Operation'}
          </Button>
        </article>
      {/each}
    </div>
  {/if}
</section>

<style>
  .content-section,
  .operation-list {
    display: grid;
    gap: var(--space-3);
  }

  .section-head,
  .operation-top {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
  }

  .section-head {
    align-items: end;
    padding: 0 var(--space-1);
  }

  .eyebrow {
    margin: 0 0 0.2rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-subtle);
    font-size: 0.6875rem;
  }

  h3 {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 600;
    line-height: 1.2;
  }

  .operation-card,
  .empty-inline {
    padding: var(--space-4);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-xl);
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--bg-card) 96%, white 4%),
      var(--bg-card)
    );
    box-shadow: var(--shadow-card);
  }

  .empty-inline {
    border-style: dashed;
    background: color-mix(in srgb, var(--bg-card) 62%, transparent);
    box-shadow: none;
    color: var(--text-muted);
  }

  .operation-list {
    gap: 0.5rem;
  }

  .operation-card {
    display: grid;
    gap: var(--space-3);
  }

  .operation-card p {
    margin: 0;
    color: var(--text-muted);
  }

  .operation-metrics {
    display: grid;
    gap: var(--space-2);
    padding-top: var(--space-3);
    border-top: 1px solid var(--border-subtle);
  }

  .three-up {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .operation-metrics > div {
    min-width: 0;
    padding: var(--space-3);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: var(--bg-soft);
  }

  .operation-metrics span {
    display: block;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .operation-metrics strong,
  .operation-top strong {
    color: var(--text-strong);
  }

  .operation-metrics strong {
    display: block;
    overflow-wrap: anywhere;
    font-size: 0.92rem;
    line-height: 1.25;
  }

  .operation-card :global(button) {
    justify-self: end;
  }

  @media (max-width: 820px) {
    .three-up {
      grid-template-columns: 1fr;
    }

    .section-head,
    .operation-top {
      flex-direction: column;
      align-items: flex-start;
    }

    .operation-card {
      padding: var(--space-3);
    }

    .operation-card :global(button) {
      width: 100%;
      justify-self: stretch;
    }
  }
</style>
