<script lang="ts">
  import type { OperationSummary } from '@entities/operation';
  import { formatLabel } from '@entities/component';
  import { statusBadgeVariant } from '@entities/operation';
  import {
    Badge,
    Button,
    Card,
    CardContent,
    Empty,
    EmptyDescription,
    EmptyHeader,
    EmptyTitle,
  } from '@shared/ui';
  import { cn } from '@shared/classnames';
  import { formatTimestamp } from '@shared/format';

  type CanRollback = (status: string) => boolean;
  type RollbackHandler = (operationId: string) => void | Promise<void>;
  type TimestampValue = Parameters<typeof formatTimestamp>[0] | null | undefined;

  const EMPTY_VALUE = '—';

  type Props = {
    operations?: OperationSummary[];
    busy?: boolean;
    canRollback?: CanRollback;
    onRollback?: RollbackHandler;
  };

  const {
    operations = [],
    busy = false,
    canRollback = () => false,
    onRollback = () => undefined,
  }: Props = $props();

  let pendingOperationId = $state<string | null>(null);

  const entriesLabel = $derived(
    `${operations.length} ${operations.length === 1 ? 'entry' : 'entries'}`,
  );

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
</script>

<section class="grid gap-3" aria-labelledby="operation-history-title">
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div class="grid gap-1">
      <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">History</p>
      <h3 id="operation-history-title" class="text-base/5 font-semibold text-foreground">
        History
      </h3>
    </div>
    <Badge variant="outline">{entriesLabel}</Badge>
  </div>

  {#if operations.length === 0}
    <Empty>
      <EmptyHeader>
        <EmptyTitle>No history yet</EmptyTitle>
        <EmptyDescription>No operations have been recorded for this game yet.</EmptyDescription>
      </EmptyHeader>
    </Empty>
  {:else}
    <div class="grid gap-2">
      {#each operations as operation (operation.operation_id)}
        {@const titleId = getOperationTitleId(operation.operation_id)}
        {@const rollback = getRollbackState(operation)}

        <article>
          <Card aria-labelledby={titleId}>
            <CardContent>
              <div
                class={cn(
                  'flex items-center justify-between gap-4',
                  'max-lg:flex-col max-lg:items-start',
                )}
              >
                <div class="min-w-0">
                  <strong id={titleId} class="text-foreground"
                    >{formatOptionalLabel(operation.kind)}</strong
                  >

                  <p class="text-muted-foreground">
                    <time datetime={toDateTimeValue(operation.created_at)}>
                      {formatOptionalTimestamp(operation.created_at)}
                    </time>
                  </p>
                </div>

                <Badge variant={statusBadgeVariant(operation.status)}>
                  {formatOptionalLabel(operation.status)}
                </Badge>
              </div>

              <dl
                class={cn('grid grid-cols-3 gap-x-4 gap-y-3', 'max-lg:grid-cols-1')}
                aria-label="Operation details"
              >
                <div class="grid min-w-0 gap-1">
                  <dt class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
                    Completed
                  </dt>
                  <dd class="text-sm/5 font-semibold text-foreground">
                    {#if operation.completed_at != null}
                      <time datetime={toDateTimeValue(operation.completed_at)}>
                        {formatOptionalTimestamp(operation.completed_at)}
                      </time>
                    {:else}
                      {EMPTY_VALUE}
                    {/if}
                  </dd>
                </div>

                <div class="grid min-w-0 gap-1">
                  <dt class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
                    Backup status
                  </dt>
                  <dd class="text-sm/5 font-semibold text-foreground">
                    {formatOptionalLabel(operation.backup_status)}
                  </dd>
                </div>

                <div class="grid min-w-0 gap-1">
                  <dt class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
                    Items
                  </dt>
                  <dd class="text-sm/5 font-semibold text-foreground">{operation.item_count}</dd>
                </div>
              </dl>

              <Button
                variant="secondary"
                size="sm"
                disabled={rollback.isDisabled}
                aria-label={`Rollback ${formatOptionalLabel(operation.kind)} operation`}
                onclick={() => {
                  void rollbackOperation(operation);
                }}
              >
                {rollback.label}
              </Button>
            </CardContent>
          </Card>
        </article>
      {/each}
    </div>
  {/if}
</section>
