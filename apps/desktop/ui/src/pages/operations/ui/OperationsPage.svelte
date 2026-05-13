<script lang="ts">
  import type { GameSummary } from '@entities/game';
  import { type OperationHandler } from '@entities/operation';
  import { Badge, Button, Card, CardContent, CardTitle, ScrollArea } from '@shared/ui';
  import {
    createOperationViewModel,
    type OperationHistoryDetails,
    type OperationViewModel,
  } from '../model/operations-page-presenter';
  import { cn } from '@shared/classnames';

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

<section class="grid min-h-0 gap-4">
  <header class="flex flex-wrap items-start justify-between gap-3">
    <div class="grid gap-1">
      <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">Operations</p>
      <h1 class="text-2xl/tight font-semibold text-foreground">Operations</h1>
      <p class="text-sm text-muted-foreground">{pageSubtitle}</p>
    </div>
    {#if canViewGame}
      <Button variant="secondary" size="sm" onclick={handleViewGame}>View Game</Button>
    {/if}
  </header>

  <div class="min-h-0">
    <ScrollArea>
      {#if canViewGame}
        <!-- handled by PageHeader actions -->
      {/if}

      {#if details === null}
        <Card>
          <CardContent role="status" aria-live="polite">
            <p>Loading operation history...</p>
          </CardContent>
        </Card>
      {:else if !hasOperations}
        <Card>
          <CardContent>
            <CardTitle>No operations recorded yet</CardTitle>
          </CardContent>
        </Card>
      {:else}
        <div
          class="flex flex-col gap-3 pb-5"
          aria-label="Operation history"
          aria-busy={isInteractionBusy}
        >
          {#each operations as operation (operation.id)}
            <article aria-label={operation.ariaLabel}>
              <Card>
                <CardContent>
                  <div
                    class={cn(
                      'mb-3 flex items-center justify-between gap-3',
                      'max-sm:flex-col max-sm:items-start max-sm:gap-1',
                    )}
                  >
                    <div class="flex min-w-0 flex-wrap items-center gap-2">
                      <Badge variant={operation.badgeVariant}>
                        {operation.statusLabel}
                      </Badge>

                      <span class="font-semibold text-foreground">{operation.kindLabel}</span>
                    </div>

                    <div class="shrink-0 max-sm:shrink">
                      <span class="text-sm text-muted-foreground">{operation.createdAtText}</span>
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
                        'grid-cols-[repeat(auto-fit,minmax(120px,1fr))] gap-x-4 gap-y-3',
                      )}
                    >
                      <div class="grid min-w-0 gap-1">
                        <p
                          class="text-xs font-medium tracking-wider text-muted-foreground uppercase"
                        >
                          Items
                        </p>
                        <p class="text-sm/5 font-semibold text-foreground">{operation.itemCount}</p>
                      </div>

                      <div class="grid min-w-0 gap-1">
                        <p
                          class="text-xs font-medium tracking-wider text-muted-foreground uppercase"
                        >
                          Backups
                        </p>
                        <p class="text-sm/5 font-semibold text-foreground">
                          {operation.backupSummary}
                        </p>
                      </div>
                    </dl>

                    {#if operation.canRollback}
                      <div class={cn('flex shrink-0 justify-end', 'max-sm:justify-stretch')}>
                        <Button
                          variant="destructive"
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
                    <div class="mt-3">
                      <span class="text-xs text-muted-foreground"
                        >{operation.completedDurationText}</span
                      >
                    </div>
                  {/if}
                </CardContent>
              </Card>
            </article>
          {/each}
        </div>
      {/if}
    </ScrollArea>
  </div>
</section>
