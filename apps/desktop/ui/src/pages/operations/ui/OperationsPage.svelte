<script lang="ts">
  import type { GameSummary } from '@entities/game';
  import { type OperationHandler } from '@entities/operation';
  import {
    Badge,
    Button,
    DefinitionMetric,
    EmptyStatePanel,
    SectionHeader,
    Surface,
  } from '@shared/ui';
  import {
    createOperationViewModel,
    type OperationHistoryDetails,
    type OperationViewModel,
  } from '../model/operations-page-presenter';
  import { cn } from '@shared/utils';

  type Props = {
    gameCard?: GameSummary | null;
    details?: OperationHistoryDetails | null;
    busy?: boolean;
    busyOperationId?: string | null;
    onRollback?: OperationHandler;
    onViewGame?: () => void;
  };

  const EMPTY_OPERATIONS: readonly OperationViewModel[] = [];

  const {
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

<div class="flex h-full min-h-0 flex-col gap-5">
  <header class="border-b border-border-subtle pb-4">
    <SectionHeader title="Operations" titleTag="h1" description={pageSubtitle} class="px-0">
      {#if canViewGame}
        <Button variant="secondary" size="sm" onclick={handleViewGame}>View Game</Button>
      {/if}
    </SectionHeader>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if details === null}
      <div class="p-5 text-center text-text-muted" role="status" aria-live="polite">
        <p class="">Loading operation history...</p>
      </div>
    {:else if !hasOperations}
      <EmptyStatePanel class="text-center">
        <p>No operations recorded yet.</p>
      </EmptyStatePanel>
    {:else}
      <div
        class="flex flex-col gap-3 pb-5"
        aria-label="Operation history"
        aria-busy={isInteractionBusy}
      >
        {#each operations as operation (operation.id)}
          <article class="flex min-w-0 flex-col" aria-label={operation.ariaLabel}>
            <Surface>
              <div class="p-4">
                <div
                  class={cn(
                    'mb-3 flex items-center justify-between gap-3',
                    'max-sm:flex-col max-sm:items-start max-sm:gap-1',
                  )}
                >
                  <div class="flex min-w-0 flex-wrap items-center gap-2">
                    <Badge tone={operation.tone} surface="outline">
                      {operation.statusLabel}
                    </Badge>

                    <span class="font-semibold text-text-strong">{operation.kindLabel}</span>
                  </div>

                  <div class="shrink-0 max-sm:shrink">
                    <span class="text-sm text-text-muted">{operation.createdAtText}</span>
                  </div>
                </div>

                <div
                  class={cn(
                    'flex items-end justify-between gap-3',
                    'max-sm:flex-col max-sm:items-stretch',
                  )}
                >
                  <dl
                    class={cn(
                      'grid min-w-0 flex-1',
                      'grid-cols-[repeat(auto-fit,minmax(120px,1fr))] gap-3',
                    )}
                  >
                    <DefinitionMetric label="Items">
                      {operation.itemCount}
                    </DefinitionMetric>

                    <DefinitionMetric label="Backups">
                      {operation.backupSummary}
                    </DefinitionMetric>
                  </dl>

                  {#if operation.canRollback}
                    <div class={cn('flex shrink-0 justify-end', 'max-sm:justify-stretch')}>
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
                  <div class="mt-3 border-t border-border-subtle pt-2">
                    <span class="text-xs text-text-muted">{operation.completedDurationText}</span>
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
