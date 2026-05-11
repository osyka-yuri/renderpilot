<script lang="ts">
  import type { GameSummary } from '@entities/game';
  import { type OperationHandler } from '@entities/operation';
  import { Badge, Button, Surface } from '@shared/ui';
  import {
    createOperationViewModel,
    type OperationHistoryDetails,
    type OperationViewModel,
  } from '../model/operations-page-presenter';

  type Props = {
    gameCard?: GameSummary | null;
    details?: OperationHistoryDetails | null;
    busy?: boolean;
    busyOperationId?: string | null;
    onRollback?: OperationHandler;
    onViewGame?: () => void;
  };

  const EMPTY_OPERATIONS: readonly OperationViewModel[] = [];

  let {
    gameCard = null,
    details = null,
    busy = false,
    busyOperationId = null,
    onRollback,
    onViewGame,
  }: Props = $props();

  const hasRollbackHandler = $derived(typeof onRollback === 'function');
  const canViewGame = $derived(gameCard !== null && typeof onViewGame === 'function');
  const pageSubtitle = $derived(
    gameCard === null ? 'Full system activity log' : `History for ${gameCard.title}`,
  );

  const isInteractionBusy = $derived(busy || busyOperationId !== null);

  const operations = $derived.by((): readonly OperationViewModel[] => {
    if (details === null) {
      return EMPTY_OPERATIONS;
    }

    return details.operations.map((operation) =>
      createOperationViewModel(operation, {
        busyOperationId,
        isInteractionBusy,
        hasRollbackHandler,
      }),
    );
  });

  const hasOperations = $derived(operations.length > 0);

  function handleRollback(operationId: string): void {
    if (isInteractionBusy || typeof onRollback !== 'function') {
      return;
    }

    onRollback(operationId);
  }

  function handleViewGame(): void {
    if (typeof onViewGame !== 'function') {
      return;
    }

    onViewGame();
  }
</script>

<div class="operations-page">
  <header class="page-header">
    <div class="header-content">
      <div class="title-group">
        <h1 class="page-title">Operations</h1>
        <p class="page-subtitle">{pageSubtitle}</p>
      </div>

      {#if canViewGame}
        <Button variant="secondary" size="sm" onclick={handleViewGame}>View Game</Button>
      {/if}
    </div>
  </header>

  <div class="page-content">
    {#if details === null}
      <div class="loading-state" role="status" aria-live="polite">
        <p>Loading operation history...</p>
      </div>
    {:else if !hasOperations}
      <div class="empty-state-wrapper">
        <Surface tone="sunken">
          <div class="empty-state-content">
            <p>No operations recorded yet.</p>
          </div>
        </Surface>
      </div>
    {:else}
      <div class="operations-list" aria-label="Operation history" aria-busy={isInteractionBusy}>
        {#each operations as operation (operation.id)}
          <article class="operation-item" aria-label={operation.ariaLabel}>
            <Surface>
              <div class="operation-card">
                <div class="operation-header">
                  <div class="kind-group">
                    <Badge tone={operation.tone} surface="outline">
                      {operation.statusLabel}
                    </Badge>

                    <span class="kind-label">{operation.kindLabel}</span>
                  </div>

                  <div class="date-group">
                    <span class="timestamp">{operation.createdAtText}</span>
                  </div>
                </div>

                <div class="operation-body">
                  <div class="summary-grid">
                    <div class="stat">
                      <span class="stat-label">Items</span>
                      <span class="stat-value">{operation.itemCount}</span>
                    </div>

                    <div class="stat">
                      <span class="stat-label">Backups</span>
                      <span class="stat-value">{operation.backupSummary}</span>
                    </div>
                  </div>

                  {#if operation.canRollback}
                    <div class="actions">
                      <Button
                        variant="danger"
                        size="sm"
                        disabled={operation.isRollbackDisabled}
                        onclick={() => {
                          handleRollback(operation.id);
                        }}
                      >
                        {operation.rollbackLabel}
                      </Button>
                    </div>
                  {/if}
                </div>

                {#if operation.completedDurationText !== null}
                  <div class="operation-footer">
                    <span class="duration">{operation.completedDurationText}</span>
                  </div>
                {/if}
              </div>
            </Surface>
          </article>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .operations-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-xl);
    height: 100%;
    min-height: 0;
  }

  .page-header {
    border-bottom: 1px solid var(--color-border-muted);
    padding-bottom: var(--space-lg);
  }

  .header-content {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: var(--space-md);
  }

  .title-group {
    min-width: 0;
  }

  .page-title {
    font-size: var(--font-size-2xl);
    font-weight: 700;
    margin: 0;
  }

  .page-subtitle {
    color: var(--color-text-muted);
    margin: var(--space-xs) 0 0;
    overflow-wrap: anywhere;
  }

  .page-content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .loading-state {
    text-align: center;
    color: var(--color-text-muted);
    padding: var(--space-xl);
  }

  .loading-state p {
    margin: 0;
  }

  .empty-state-wrapper {
    text-align: center;
    color: var(--color-text-muted);
  }

  .empty-state-content {
    padding: var(--space-xl);
  }

  .empty-state-content p {
    margin: 0;
  }

  .operations-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-md);
    padding-bottom: var(--space-xl);
  }

  .operation-item {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .operation-card {
    padding: var(--space-lg);
  }

  .operation-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-md);
    margin-bottom: var(--space-md);
  }

  .kind-group {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-sm);
    min-width: 0;
  }

  .kind-label {
    font-weight: 600;
    color: var(--color-text-emphasis);
  }

  .date-group {
    flex-shrink: 0;
  }

  .timestamp {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .operation-body {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: var(--space-md);
  }

  .summary-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: var(--space-md);
    flex: 1;
    min-width: 0;
    background: var(--color-surface-sunken);
    padding: var(--space-sm) var(--space-md);
    border-radius: var(--radius-md);
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: var(--space-xs);
    min-width: 0;
  }

  .stat-label {
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-text-muted);
  }

  .stat-value {
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    flex-shrink: 0;
  }

  .operation-footer {
    border-top: 1px solid var(--color-border-muted);
    padding-top: var(--space-sm);
    margin-top: var(--space-md);
  }

  .duration {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  @media (max-width: 640px) {
    .header-content {
      flex-direction: column;
      align-items: stretch;
    }

    .operation-header {
      flex-direction: column;
      align-items: flex-start;
      gap: var(--space-xs);
    }

    .date-group {
      flex-shrink: 1;
    }

    .operation-body {
      flex-direction: column;
      align-items: stretch;
    }

    .actions {
      justify-content: stretch;
    }
  }
</style>
