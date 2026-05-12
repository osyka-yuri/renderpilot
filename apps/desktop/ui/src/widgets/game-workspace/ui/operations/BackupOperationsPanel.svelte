<script lang="ts">
  import type { OperationSummary } from '@entities/operation';
  import { formatLabel } from '@entities/component';
  import { statusTone } from '@entities/operation';
  import { Badge, DefinitionMetric, EmptyStatePanel, SectionHeader, Surface } from '@shared/ui';
  import { cn, formatTimestamp } from '@shared/utils';

  type Props = {
    operations?: OperationSummary[];
  };

  const { operations = [] }: Props = $props();
</script>

<section class="grid gap-3">
  <SectionHeader eyebrow="Backups" title="Backups">
    <Badge surface="outline" tone="muted">{operations.length} backup-ready operations</Badge>
  </SectionHeader>

  {#if operations.length === 0}
    <EmptyStatePanel>No backup snapshots exist yet for this game installation.</EmptyStatePanel>
  {:else}
    <div class="grid gap-2">
      {#each operations as operation (operation.kind + String(operation.completed_at ?? operation.created_at))}
        <Surface as="article" shadow class={cn('grid gap-3 p-4', 'max-lg:p-3')}>
          <div
            class={cn(
              'flex items-center justify-between gap-4',
              'max-lg:flex-col max-lg:items-start',
            )}
          >
            <strong class="text-text-strong">{formatLabel(operation.kind)}</strong>
            <Badge pill tone={statusTone(operation.status)}>{formatLabel(operation.status)}</Badge>
          </div>
          <p class="text-text-muted">
            {formatTimestamp(operation.completed_at ?? operation.created_at)}
          </p>
          <dl
            class={cn(
              'grid grid-cols-2 gap-2 border-t border-border-subtle pt-3',
              'max-lg:grid-cols-1',
            )}
          >
            <DefinitionMetric label="Backup status">
              {formatLabel(operation.backup_status)}
            </DefinitionMetric>
            <DefinitionMetric label="Snapshots">{operation.backup_count}</DefinitionMetric>
          </dl>
        </Surface>
      {/each}
    </div>
  {/if}
</section>
