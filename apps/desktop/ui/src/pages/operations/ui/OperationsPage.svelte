<script lang="ts">
  import type { GameSummary } from '@entities/game';
  import {
    Badge,
    Card,
    CardContent,
    CardTitle,
    ScrollArea,
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
    createSvelteTable,
    FlexRender,
    renderSnippet,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import {
    createOperationViewModel,
    type OperationHistoryDetails,
    type OperationViewModel,
  } from '../model/operations-page-presenter';
  import { type ColumnDef, getCoreRowModel } from '@tanstack/table-core';

  const EMPTY_VALUE = '—';

  type Props = {
    gameCard?: GameSummary | null;
    details?: OperationHistoryDetails | null;
  };

  const EMPTY_OPERATIONS: OperationViewModel[] = [];

  const { gameCard = null, details = null }: Props = $props();

  const pageSubtitle = $derived(
    gameCard === null ? undefined : t('operations.subtitleGame', { title: gameCard.title }),
  );

  const operations = $derived.by((): OperationViewModel[] => {
    if (details === null) {
      return EMPTY_OPERATIONS;
    }

    return details.operations.map((op) => createOperationViewModel(op, gameCard));
  });

  const hasOperations = $derived(operations.length > 0);

  const columns = $derived.by((): ColumnDef<OperationViewModel>[] => {
    const baseColumns: ColumnDef<OperationViewModel>[] = [
      {
        id: 'date',
        header: () => t('operations.date'),
        cell: ({ row }) => renderSnippet(dateCell, row.original),
      },
      {
        accessorKey: 'statusLabel',
        header: () => t('operations.status'),
        cell: ({ row }) => renderSnippet(statusCell, row.original),
      },
      {
        accessorKey: 'kindLabel',
        header: () => t('operations.action'),
        cell: ({ row }) => renderSnippet(actionCell, row.original),
      },
      {
        accessorKey: 'libraryType',
        header: () => t('operations.libraryType'),
        cell: ({ row }) => renderSnippet(libraryCell, row.original),
      },
      {
        id: 'version',
        header: () => t('operations.version'),
        cell: ({ row }) => renderSnippet(versionCell, row.original),
      },
    ];

    if (gameCard !== null) {
      return baseColumns;
    }

    return [
      {
        accessorKey: 'gameName',
        header: () => t('operations.gameName'),
        cell: ({ row }) => renderSnippet(gameNameCell, row.original),
      },
      ...baseColumns,
    ];
  });

  const table = createSvelteTable({
    get data() {
      return operations;
    },
    get columns() {
      return columns;
    },
    getCoreRowModel: getCoreRowModel(),
  });
</script>

{#snippet gameNameCell(operation: OperationViewModel)}
  <span class="font-medium">{operation.gameName}</span>
{/snippet}

{#snippet dateCell(operation: OperationViewModel)}
  <span>{operation.createdAtText}</span>
{/snippet}

{#snippet statusCell(operation: OperationViewModel)}
  <Badge variant={operation.badgeVariant}>{operation.statusLabel}</Badge>
{/snippet}

{#snippet actionCell(operation: OperationViewModel)}
  <span>{operation.kindLabel}</span>
{/snippet}

{#snippet libraryCell(operation: OperationViewModel)}
  <span class="font-medium">{operation.libraryType}</span>
{/snippet}

{#snippet versionCell(operation: OperationViewModel)}
  <div class="flex items-center gap-1.5 whitespace-nowrap">
    <span class="text-muted-foreground">{operation.fromVersion ?? EMPTY_VALUE}</span>
    <span class="text-muted-foreground">→</span>
    <span class="font-medium">{operation.toVersion ?? EMPTY_VALUE}</span>
  </div>
{/snippet}

<section class="grid h-full min-h-0 grid-rows-[auto_1fr] gap-4 overflow-hidden">
  <header class="flex flex-wrap items-start justify-between gap-3">
    <div class="grid gap-1">
      <h1 class="text-2xl/tight font-semibold text-foreground">{t('operations.title')}</h1>
      {#if pageSubtitle}
        <p class="text-sm text-muted-foreground">{pageSubtitle}</p>
      {/if}
    </div>
  </header>

  <div class="min-h-0">
    {#if details === null}
      <Card>
        <CardContent role="status" aria-live="polite">
          <p>{t('operations.loading')}</p>
        </CardContent>
      </Card>
    {:else if !hasOperations}
      <Card>
        <CardContent>
          <CardTitle>{t('operations.empty')}</CardTitle>
        </CardContent>
      </Card>
    {:else}
      <ScrollArea orientation="both" class="h-full">
        <Table>
          <TableHeader>
            {#each table.getHeaderGroups() as headerGroup (headerGroup.id)}
              <TableRow>
                {#each headerGroup.headers as header (header.id)}
                  <TableHead>
                    {#if !header.isPlaceholder}
                      <FlexRender
                        content={header.column.columnDef.header}
                        context={header.getContext()}
                      />
                    {/if}
                  </TableHead>
                {/each}
              </TableRow>
            {/each}
          </TableHeader>
          <TableBody>
            {#each table.getRowModel().rows as row (row.id)}
              <TableRow>
                {#each row.getVisibleCells() as cell (cell.id)}
                  <TableCell>
                    <FlexRender content={cell.column.columnDef.cell} context={cell.getContext()} />
                  </TableCell>
                {/each}
              </TableRow>
            {/each}
          </TableBody>
        </Table>
      </ScrollArea>
    {/if}
  </div>
</section>
