<script lang="ts">
  import type { OperationSummary } from '@entities/operation';
  import { formatLabel } from '@entities/component';
  import { statusTone } from '@entities/operation';
  import {
    Badge,
    Button,
    DefinitionMetric,
    EmptyStatePanel,
    SectionHeader,
    Surface,
  } from '@shared/ui';
  import { cn, formatTimestamp } from '@shared/utils';

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
  <SectionHeader eyebrow="History" title="History" titleId="operation-history-title">
    <svelte:fragment>
      <Badge surface="outline" tone="muted">{entriesLabel}</Badge>
    </svelte:fragment>
  </SectionHeader>

  {#if operations.length === 0}
    <EmptyStatePanel>No operations have been recorded for this game yet.</EmptyStatePanel>
  {:else}
    <div class="grid gap-2">
      {#each operations as operation (operation.operation_id)}
        {@const titleId = getOperationTitleId(operation.operation_id)}
        {@const rollback = getRollbackState(operation)}

        <Surface
          as="article"
          shadow
          class={cn(
            'grid gap-3 p-4',
            '*:last:justify-self-end',
            'max-lg:p-3',
            'max-lg:*:last:w-full max-lg:*:last:justify-self-stretch',
          )}
          aria-labelledby={titleId}
        >
          <div
            class={cn(
              'flex items-center justify-between gap-4',
              'max-lg:flex-col max-lg:items-start',
            )}
          >
            <div class="min-w-0">
              <strong id={titleId} class="text-text-strong"
                >{formatOptionalLabel(operation.kind)}</strong
              >

              <p class="text-text-muted">
                <time datetime={toDateTimeValue(operation.created_at)}>
                  {formatOptionalTimestamp(operation.created_at)}
                </time>
              </p>
            </div>

            <Badge pill tone={statusTone(operation.status)}>
              {formatOptionalLabel(operation.status)}
            </Badge>
          </div>

          <dl
            class={cn(
              'grid grid-cols-3 gap-2 border-t border-border-subtle pt-3',
              'max-lg:grid-cols-1',
            )}
            aria-label="Operation details"
          >
            <DefinitionMetric label="Completed">
              {#if operation.completed_at != null}
                <time datetime={toDateTimeValue(operation.completed_at)}>
                  {formatOptionalTimestamp(operation.completed_at)}
                </time>
              {:else}
                {EMPTY_VALUE}
              {/if}
            </DefinitionMetric>

            <DefinitionMetric label="Backup status">
              {formatOptionalLabel(operation.backup_status)}
            </DefinitionMetric>

            <DefinitionMetric label="Items">{operation.item_count}</DefinitionMetric>
          </dl>

          <Button
            variant="secondary"
            size="sm"
            disabled={rollback.isDisabled}
            loading={rollback.isLoading}
            aria-label={`Rollback ${formatOptionalLabel(operation.kind)} operation`}
            onclick={() => {
              void rollbackOperation(operation);
            }}
          >
            {rollback.label}
          </Button>
        </Surface>
      {/each}
    </div>
  {/if}
</section>
