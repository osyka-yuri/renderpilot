<script lang="ts">
  import type { OperationSummary } from '@entities/operation';
  import { formatLabel } from '@entities/component';
  import { statusBadgeVariant } from '@entities/operation';
  import {
    Badge,
    Card,
    CardContent,
    Empty,
    EmptyDescription,
    EmptyHeader,
    EmptyTitle,
  } from '@shared/ui';
  import { cn } from '@shared/classnames';
  import { formatTimestamp } from '@shared/format';

  type Props = {
    operations?: OperationSummary[];
  };

  const { operations = [] }: Props = $props();
</script>

<section class="grid gap-3">
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div class="grid gap-1">
      <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">Backups</p>
      <h3 class="text-base/5 font-semibold text-foreground">Backups</h3>
    </div>
    <Badge variant="outline">{operations.length} backup-ready operations</Badge>
  </div>

  {#if operations.length === 0}
    <Empty>
      <EmptyHeader>
        <EmptyTitle>No backups yet</EmptyTitle>
        <EmptyDescription>
          No backup snapshots exist yet for this game installation.
        </EmptyDescription>
      </EmptyHeader>
    </Empty>
  {:else}
    <div class="grid gap-2">
      {#each operations as operation (operation.kind + String(operation.completed_at ?? operation.created_at))}
        <article>
          <Card>
            <CardContent>
              <div
                class={cn(
                  'flex items-center justify-between gap-4',
                  'max-lg:flex-col max-lg:items-start',
                )}
              >
                <strong class="text-foreground">{formatLabel(operation.kind)}</strong>
                <Badge variant={statusBadgeVariant(operation.status)}>
                  {formatLabel(operation.status)}
                </Badge>
              </div>
              <p class="text-muted-foreground">
                {formatTimestamp(operation.completed_at ?? operation.created_at)}
              </p>
              <dl class={cn('grid grid-cols-2 gap-x-4 gap-y-3', 'max-lg:grid-cols-1')}>
                <div class="grid min-w-0 gap-1">
                  <dt class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
                    Backup status
                  </dt>
                  <dd class="text-sm/5 font-semibold text-foreground">
                    {formatLabel(operation.backup_status)}
                  </dd>
                </div>
                <div class="grid min-w-0 gap-1">
                  <dt class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
                    Snapshots
                  </dt>
                  <dd class="text-sm/5 font-semibold text-foreground">{operation.backup_count}</dd>
                </div>
              </dl>
            </CardContent>
          </Card>
        </article>
      {/each}
    </div>
  {/if}
</section>
