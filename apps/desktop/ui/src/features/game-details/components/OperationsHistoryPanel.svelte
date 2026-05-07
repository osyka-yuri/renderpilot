<script lang="ts">
  import type { OperationSummary } from '@shared/api/types';
  import { formatLabel, formatTimestamp, statusTone } from '@shared/utils/presenters';
  import Badge from '@shared/ui/Badge.svelte';
  import Button from '@shared/ui/Button.svelte';

  type CanRollback = (status: string) => boolean;
  type RollbackHandler = (operationId: string) => void | Promise<void>;
  type TimestampValue = Parameters<typeof formatTimestamp>[0] | null | undefined;

  const EMPTY_VALUE = '—';

  export let operations: OperationSummary[] = [];
  export let busy = false;
  export let canRollback: CanRollback = () => false;
  export let onRollback: RollbackHandler = () => undefined;

  let pendingOperationId: string | null = null;

  $: entriesLabel = `${operations.length} ${operations.length === 1 ? 'entry' : 'entries'}`;

  function formatOptionalLabel(value: string | null | undefined): string {
    return value ? formatLabel(value) : EMPTY_VALUE;
  }

  function formatOptionalTimestamp(value: TimestampValue): string {
    return value == null ? EMPTY_VALUE : formatTimestamp(value);
  }

  function toDateTimeValue(value: TimestampValue): string | undefined {
    if (value == null) {
      return undefined;
    }

    const normalizedValue =
      typeof value === 'number' && value > 0 && value < 1_000_000_000_000 ? value * 1000 : value;

    const date = new Date(normalizedValue);

    return Number.isNaN(date.getTime()) ? undefined : date.toISOString();
  }

  function canRollbackSafely(status: string): boolean {
    try {
      return canRollback(status);
    } catch {
      return false;
    }
  }

  function getOperationTitleId(operationId: string): string {
    return `operation-title-${operationId.replace(/[^a-zA-Z0-9_-]/g, '-')}`;
  }

  function getOperationById(operationId: string): OperationSummary | undefined {
    return operations.find((operation) => operation.operation_id === operationId);
  }

  function getRollbackState(operation: OperationSummary) {
    const isPending = pendingOperationId === operation.operation_id;
    const rollbackAllowed = canRollbackSafely(operation.status);
    const interactionLocked = busy || pendingOperationId !== null;

    return {
      isDisabled: interactionLocked || !rollbackAllowed,
      isLoading: isPending || busy,
      label: isPending || busy ? 'Working...' : 'Rollback This Operation',
    };
  }

  async function rollbackOperation(operation: OperationSummary): Promise<void> {
    if (busy || pendingOperationId || !canRollbackSafely(operation.status)) {
      return;
    }

    pendingOperationId = operation.operation_id;

    try {
      await Promise.resolve(onRollback(operation.operation_id));
    } finally {
      pendingOperationId = null;
    }
  }

  function handleRollbackClick(event: MouseEvent): void {
    const button = event.currentTarget as HTMLElement;
    const operationId = button.dataset.operationId;

    if (!operationId) {
      return;
    }

    const operation = getOperationById(operationId);

    if (!operation) {
      return;
    }

    void rollbackOperation(operation);
  }
</script>

<section class="content-section" aria-labelledby="operation-history-title">
  <div class="section-head">
    <div>
      <p class="eyebrow">History</p>
      <h3 id="operation-history-title">History</h3>
    </div>

    <Badge surface="outline" tone="muted">{entriesLabel}</Badge>
  </div>

  {#if operations.length === 0}
    <div class="empty-inline">No operations have been recorded for this game yet.</div>
  {:else}
    <div class="operation-list">
      {#each operations as operation (operation.operation_id)}
        {@const titleId = getOperationTitleId(operation.operation_id)}
        {@const rollback = getRollbackState(operation)}

        <article class="operation-card" aria-labelledby={titleId}>
          <div class="operation-top">
            <div class="operation-summary">
              <strong id={titleId}>{formatOptionalLabel(operation.kind)}</strong>

              <p>
                <time datetime={toDateTimeValue(operation.created_at)}>
                  {formatOptionalTimestamp(operation.created_at)}
                </time>
              </p>
            </div>

            <Badge pill tone={statusTone(operation.status)}>
              {formatOptionalLabel(operation.status)}
            </Badge>
          </div>

          <dl class="operation-metrics three-up" aria-label="Operation details">
            <div>
              <dt>Completed</dt>
              <dd>
                {#if operation.completed_at != null}
                  <time datetime={toDateTimeValue(operation.completed_at)}>
                    {formatOptionalTimestamp(operation.completed_at)}
                  </time>
                {:else}
                  {EMPTY_VALUE}
                {/if}
              </dd>
            </div>

            <div>
              <dt>Backup status</dt>
              <dd>{formatOptionalLabel(operation.backup_status)}</dd>
            </div>

            <div>
              <dt>Items</dt>
              <dd>{operation.item_count}</dd>
            </div>
          </dl>

          <Button
            variant="secondary"
            size="sm"
            disabled={rollback.isDisabled}
            loading={rollback.isLoading}
            data-operation-id={operation.operation_id}
            aria-label={`Rollback ${formatOptionalLabel(operation.kind)} operation`}
            onclick={handleRollbackClick}
          >
            {rollback.label}
          </Button>
        </article>
      {/each}
    </div>
  {/if}
</section>

<style>
  .content-section,
  .operation-list,
  .operation-card {
    display: grid;
  }

  .content-section {
    gap: var(--space-3);
  }

  .operation-list {
    gap: 0.5rem;
  }

  .operation-card {
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

  .section-head > div,
  .operation-summary {
    min-width: 0;
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
  }

  .operation-card {
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

  .operation-card p {
    margin: 0;
    color: var(--text-muted);
  }

  .operation-metrics {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-3) 0 0;
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

  .operation-metrics dt {
    color: var(--text-subtle);
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .operation-metrics dd {
    margin: 0;
    overflow-wrap: anywhere;
    color: var(--text-strong);
    font-size: 0.92rem;
    font-weight: 600;
    line-height: 1.25;
  }

  .operation-top strong {
    color: var(--text-strong);
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
