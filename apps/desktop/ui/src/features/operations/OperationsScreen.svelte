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

  const noop: VoidHandler = (): void => {};
  const noopOperation: OperationHandler = (_operationId: string): void => {};

  export let details: GameDetails | null = null;
  export let gameCard: GameCard | null = null;
  export let busy = false;
  export let onRollback: OperationHandler = noopOperation;
  export let onOpenDetails: VoidHandler = noop;

  $: rollbackReadyCount =
    details?.operations.filter((operation) => canRollback(operation)).length ?? 0;

  function handleOpenDetails(): void {
    onOpenDetails();
  }

  function handleRollback(operationId: string): void {
    onRollback(operationId);
  }

  function canRollback(operation: GameDetails['operations'][number]): boolean {
    return operation.backup_count > 0 && operation.backup_status.toLowerCase() === 'available';
  }
</script>

<section class="screen-shell">
  {#if !details}
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

        <Button variant="secondary" size="sm" onclick={handleOpenDetails}>Back To Details</Button>
      </div>

      <Surface className="summary-panel" tone="elevated" shadow>
        <div class="summary-grid">
          <div class="summary-item">
            <span>Total operations</span>
            <strong>{details.operations.length}</strong>
          </div>
          <div class="summary-item">
            <span>Rollback ready</span>
            <strong>{rollbackReadyCount}</strong>
          </div>
          <div class="summary-item">
            <span>Risk context</span>
            <Badge surface="outline" tone={riskBadgeTone(gameCard?.risk_level)}>
              {formatLabel(gameCard?.risk_level ?? 'unknown')}
            </Badge>
          </div>
          <div class="summary-item">
            <span>Latest result</span>
            <strong>{formatLabel(details.operations[0]?.status ?? 'none')}</strong>
          </div>
        </div>
      </Surface>
    </section>

    {#if details.operations.length === 0}
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
          {#each details.operations as operation}
            <Surface as="article" className="operation-row" interactive>
              <div class="row-head">
                <div class="row-copy">
                  <p class="eyebrow">{formatLabel(operation.kind)}</p>
                  <h4>{operation.operation_id}</h4>
                  <p class="row-note">Created {formatTimestamp(operation.created_at)}</p>
                </div>

                <Badge pill tone={statusTone(operation.status)}>
                  {formatLabel(operation.status)}
                </Badge>
              </div>

              <div class="detail-grid">
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
                <div>
                  <span>Backups</span>
                  <strong>{operation.backup_count}</strong>
                </div>
              </div>

              <div class="row-actions">
                <p class="rollback-copy">
                  {#if canRollback(operation)}
                    Rollback is available for this entry.
                  {:else}
                    Rollback is unavailable because this entry does not have a restorable backup
                    set.
                  {/if}
                </p>

                <Button
                  variant="secondary"
                  size="sm"
                  disabled={busy || !canRollback(operation)}
                  loading={busy}
                  onclick={() => handleRollback(operation.operation_id)}
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
    justify-content: space-between;
    gap: var(--space-4);
    align-items: start;
  }

  .section-head.compact {
    padding: 0 var(--space-1);
  }

  :global(.summary-panel),
  :global(.empty-state),
  :global(.operation-row) {
    padding: var(--space-4);
  }

  .summary-grid,
  .detail-grid {
    display: grid;
    gap: var(--space-3);
  }

  .summary-grid {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }

  .detail-grid {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }

  .summary-item,
  .row-copy {
    min-width: 0;
  }

  .eyebrow {
    margin: 0 0 var(--space-1);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.6875rem;
    color: var(--text-subtle);
  }

  h3,
  h4 {
    margin: 0;
    color: var(--text-strong);
  }

  h3 {
    font-size: 1.05rem;
    font-weight: 600;
  }

  h4 {
    font-size: 0.95rem;
    font-weight: 600;
    word-break: break-word;
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
    font-size: 0.6875rem;
    color: var(--text-subtle);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .summary-item strong,
  .detail-grid strong {
    color: var(--text-strong);
  }

  :global(.operation-row) {
    display: grid;
    gap: var(--space-3);
  }

  .row-actions {
    padding-top: var(--space-3);
    border-top: 1px solid var(--border-subtle);
    align-items: center;
  }

  .rollback-copy {
    margin: 0;
  }

  :global(.empty-state) {
    border-style: dashed;
    background: color-mix(in srgb, var(--bg-card) 62%, transparent);
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
