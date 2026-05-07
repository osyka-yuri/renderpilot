<script lang="ts">
  import type { GameCard, GameDetails } from '@shared/api/types';
  import type { OperationHandler, VoidHandler } from '@shared/utils/callbacks';
  import Badge from '@shared/ui/Badge.svelte';
  import Button from '@shared/ui/Button.svelte';
  import Surface from '@shared/ui/Surface.svelte';
  import {
    formatLabel,
    formatTimestamp,
    riskBadgeTone,
    statusTone,
  } from '@shared/utils/presenters';

  type Operation = GameDetails['operations'][number];

  type OperationViewModel = {
    operation: Operation;
    canRollback: boolean;
    kindLabel: string;
    statusLabel: string;
    createdAtLabel: string;
    completedAtLabel: string;
    backupStatusLabel: string;
    rollbackCopy: string;
  };

  const BACKUP_AVAILABLE_STATUS = 'available';

  const noop: VoidHandler = (): void => {
    /* empty */
  };
  const noopOperation: OperationHandler = (_operationId: string): void => {
    /* empty */
  };

  export let details: GameDetails | null = null;
  export let gameCard: GameCard | null = null;
  export let busy = false;
  export let onRollback: OperationHandler = noopOperation;
  export let onOpenDetails: VoidHandler = noop;

  $: operations = details?.operations ?? [];
  $: operationRows = operations.map(toOperationViewModel);

  $: totalOperations = operations.length;
  $: rollbackReadyCount = operationRows.filter((row) => row.canRollback).length;
  $: latestResultLabel = formatLabel(operations[0]?.status ?? 'none');

  $: riskLevel = gameCard?.risk_level ?? 'unknown';
  $: riskLabel = formatLabel(riskLevel);
  $: riskToneValue = riskBadgeTone(gameCard?.risk_level);

  function handleOpenDetails(): void {
    onOpenDetails();
  }

  function handleRollbackClick(event: MouseEvent): void {
    if (busy) {
      return;
    }

    const target = event.currentTarget;

    if (!(target instanceof HTMLElement)) {
      return;
    }

    const operationId = target.dataset.operationId;

    if (!operationId) {
      return;
    }

    const operation = operations.find((item) => item.operation_id === operationId);

    if (!operation || !isRollbackAvailable(operation)) {
      return;
    }

    onRollback(operationId);
  }

  function toOperationViewModel(operation: Operation): OperationViewModel {
    const canRollback = isRollbackAvailable(operation);

    return {
      operation,
      canRollback,
      kindLabel: formatLabel(operation.kind),
      statusLabel: formatLabel(operation.status),
      createdAtLabel: formatTimestamp(operation.created_at),
      completedAtLabel: formatTimestamp(operation.completed_at),
      backupStatusLabel: formatLabel(operation.backup_status),
      rollbackCopy: canRollback
        ? 'Rollback is available for this entry.'
        : 'Rollback is unavailable because this entry does not have a restorable backup set.',
    };
  }

  function isRollbackAvailable(operation: Operation): boolean {
    return (
      operation.backup_count > 0 &&
      normalizeStatus(operation.backup_status) === BACKUP_AVAILABLE_STATUS
    );
  }

  function normalizeStatus(status: string | null | undefined): string {
    return status?.trim().toLowerCase() ?? '';
  }
</script>

<section class="screen-shell">
  {#if details === null}
    <Surface className="empty-state">
      <h3>No operation context</h3>
      <p>
        Select a game to inspect journal history, rollback readiness, and recovery-oriented status.
      </p>
    </Surface>
  {:else}
    <section class="section-shell">
      <div class="section-head">
        <div>
          <p class="eyebrow">Journal</p>
          <h3>Operation history</h3>
          <p class="section-copy">
            Durable entries for swaps, rollback, and recovery state for this installation.
          </p>
        </div>

        <Button variant="secondary" size="sm" onclick={handleOpenDetails}>Back to details</Button>
      </div>

      <Surface className="summary-panel" tone="elevated" shadow>
        <div class="summary-grid">
          <div class="summary-item">
            <span>Total operations</span>
            <strong>{totalOperations}</strong>
          </div>

          <div class="summary-item">
            <span>Rollback ready</span>
            <strong>{rollbackReadyCount}</strong>
          </div>

          <div class="summary-item">
            <span>Risk context</span>
            <Badge surface="outline" tone={riskToneValue}>
              {riskLabel}
            </Badge>
          </div>

          <div class="summary-item">
            <span>Latest result</span>
            <strong>{latestResultLabel}</strong>
          </div>
        </div>
      </Surface>
    </section>

    {#if operationRows.length === 0}
      <Surface className="empty-state">
        <h3>No operations yet</h3>
        <p>
          Build a plan from Game Details, then apply it. Once a swap runs, the journal and rollback
          readiness will appear here.
        </p>
      </Surface>
    {:else}
      <section class="section-shell">
        <div class="section-head compact">
          <div>
            <p class="eyebrow">Entries</p>
            <h3>Journal entries</h3>
            <p class="section-copy">
              Structured history with rollback affordances instead of a raw log dump.
            </p>
          </div>
        </div>

        <div class="operation-list">
          {#each operationRows as row (row.operation.operation_id)}
            <Surface as="article" className="operation-row" interactive>
              <div class="row-head">
                <div class="row-copy">
                  <p class="eyebrow">{row.kindLabel}</p>
                  <h4>{row.operation.operation_id}</h4>
                  <p class="row-note">Created {row.createdAtLabel}</p>
                </div>

                <Badge pill tone={statusTone(row.operation.status)}>
                  {row.statusLabel}
                </Badge>
              </div>

              <div class="detail-grid">
                <div>
                  <span>Completed</span>
                  <strong>{row.completedAtLabel}</strong>
                </div>

                <div>
                  <span>Backup status</span>
                  <strong>{row.backupStatusLabel}</strong>
                </div>

                <div>
                  <span>Items</span>
                  <strong>{row.operation.item_count}</strong>
                </div>

                <div>
                  <span>Backups</span>
                  <strong>{row.operation.backup_count}</strong>
                </div>
              </div>

              <div class="row-actions">
                <p class="rollback-copy">{row.rollbackCopy}</p>

                <Button
                  variant="secondary"
                  size="sm"
                  disabled={busy || !row.canRollback}
                  loading={busy}
                  data-operation-id={row.operation.operation_id}
                  onclick={handleRollbackClick}
                >
                  {busy ? 'Working...' : 'Rollback'}
                </Button>
              </div>
            </Surface>
          {/each}
        </div>
      </section>
    {/if}
  {/if}
</section>

<style>
  .screen-shell,
  .section-shell,
  .operation-list {
    display: grid;
    gap: var(--space-3);
  }

  .section-head,
  .row-head,
  .row-actions {
    display: flex;
    gap: var(--space-4);
    align-items: flex-start;
    justify-content: space-between;
  }

  .section-head.compact {
    padding-inline: var(--space-1);
  }

  :global(.summary-panel),
  :global(.empty-state),
  :global(.operation-row) {
    padding: var(--space-4);
  }

  :global(.operation-row) {
    display: grid;
    gap: var(--space-3);
  }

  :global(.empty-state) {
    border-style: dashed;
    background: color-mix(in srgb, var(--bg-card) 62%, transparent);
  }

  .summary-grid,
  .detail-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--space-3);
  }

  .summary-item,
  .row-copy {
    min-width: 0;
  }

  .eyebrow {
    margin: 0 0 var(--space-1);
    color: var(--text-subtle);
    font-size: 0.6875rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  h3,
  h4 {
    margin: 0;
    color: var(--text-strong);
    font-weight: 600;
  }

  h3 {
    font-size: 1.05rem;
  }

  h4 {
    overflow-wrap: anywhere;
    font-size: 0.95rem;
  }

  .section-copy,
  .row-note,
  .rollback-copy,
  :global(.empty-state) p {
    margin: var(--space-1) 0 0;
    color: var(--text-muted);
    line-height: 1.45;
  }

  .section-copy {
    max-width: 42rem;
    font-size: 0.875rem;
  }

  .row-note,
  .rollback-copy,
  :global(.empty-state) p {
    font-size: 0.8125rem;
  }

  .summary-item span,
  .detail-grid span {
    display: block;
    margin-bottom: var(--space-1);
    color: var(--text-subtle);
    font-size: 0.6875rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .summary-item strong,
  .detail-grid strong {
    color: var(--text-strong);
  }

  .row-actions {
    align-items: center;
    padding-top: var(--space-3);
    border-top: 1px solid var(--border-subtle);
  }

  .rollback-copy {
    margin: 0;
  }

  @media (max-width: 1040px) {
    .summary-grid,
    .detail-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 720px) {
    .section-head,
    .row-head,
    .row-actions {
      flex-direction: column;
      align-items: stretch;
    }

    .summary-grid,
    .detail-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
