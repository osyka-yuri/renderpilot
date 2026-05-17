<script lang="ts">
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import {
    Alert,
    AlertDescription,
    AlertTitle,
    FlexRender,
    ScrollArea,
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
    Tabs,
    TabsContent,
    TabsList,
    TabsTrigger,
    ToggleGroup,
    ToggleGroupItem,
    createSvelteTable,
  } from '@shared/ui';
  import {
    type ColumnDef,
    type Row,
    type SortingState,
    getCoreRowModel,
    getSortedRowModel,
  } from '@tanstack/table-core';

  import {
    type LibraryManifestEntry,
    typeOptionsByVendor,
    vendorOptions,
  } from '../model/libraries-page-model';
  import { createLibrariesPageModel } from '../model/create-libraries-page-model.svelte';

  import { createLibraryColumns } from './library-columns';
  import ManifestRefreshButton from './ManifestRefreshButton.svelte';
  import {
    getBottomVirtualPadding,
    getTopVirtualPadding,
    resetVirtualizerAfterLayout,
  } from './virtualizer-reset';

  const model = createLibrariesPageModel();

  const DEFAULT_SORTING: SortingState = [{ id: 'version', desc: true }];

  const ROW_ESTIMATE_SIZE = 40;
  const ROW_OVERSCAN = 12;

  const COLUMN_IDS = ['version', 'hash', 'signed', 'size', 'actions'] as const;
  const COLUMN_COUNT = COLUMN_IDS.length;

  const COLUMN_CLASS_BY_ID = {
    version: 'w-48',
    hash: 'w-64',
    signed: 'w-40',
    size: 'w-24',
    actions: 'w-24 text-end',
  } satisfies Readonly<Record<(typeof COLUMN_IDS)[number], string>>;

  let sorting = $state<SortingState>([...DEFAULT_SORTING]);
  let scrollViewportRef = $state<HTMLElement | null>(null);

  let virtualizerResetId = 0;

  const columns = $derived(
    createLibraryColumns(
      model.pendingEntryAction,
      model.downloadedEntryIds,
      model.handleDownload,
      model.handleDelete,
    ),
  );

  const table = $derived(createTable(model.filteredEntries, columns));

  const tableRows = $derived(table.getRowModel().rows);

  const rowVirtualizer = $derived.by(() => {
    const scrollElement = scrollViewportRef;
    const rows = tableRows;

    return createVirtualizer<HTMLElement, HTMLTableRowElement>({
      count: rows.length,
      getScrollElement: () => scrollElement,
      estimateSize: () => ROW_ESTIMATE_SIZE,
      overscan: ROW_OVERSCAN,
      getItemKey: (index) => getRowByIndex(rows, index)?.original.entry_id ?? index,
    });
  });

  const virtualRows = $derived($rowVirtualizer.getVirtualItems());

  const topVirtualPadding = $derived(getTopVirtualPadding(virtualRows));
  const bottomVirtualPadding = $derived(
    getBottomVirtualPadding(virtualRows, $rowVirtualizer.getTotalSize()),
  );

  $effect(() => {
    const viewport = scrollViewportRef;
    const rowCount = tableRows.length;
    const resetKey = getVirtualizerResetKey();

    if (viewport === null || rowCount === 0) return;

    scheduleVirtualizerReset(resetKey);
  });

  $effect(() => {
    model.init();
    void model.loadInitialLibraries();

    return () => {
      model.dispose();
      virtualizerResetId += 1;
    };
  });

  function createTable(
    entries: LibraryManifestEntry[],
    tableColumns: ColumnDef<LibraryManifestEntry>[],
  ) {
    return createSvelteTable({
      get data() {
        return entries;
      },
      columns: tableColumns,
      state: {
        get sorting() {
          return sorting;
        },
      },
      onSortingChange: (updater) => {
        sorting = typeof updater === 'function' ? updater(sorting) : updater;
        scheduleVirtualizerReset(getVirtualizerResetKey());
      },
      getCoreRowModel: getCoreRowModel(),
      getSortedRowModel: getSortedRowModel(),
    });
  }

  function getColumnClass(columnId: string): string {
    return COLUMN_CLASS_BY_ID[columnId as keyof typeof COLUMN_CLASS_BY_ID];
  }

  function getRowByIndex(
    rows: Row<LibraryManifestEntry>[],
    index: number,
  ): Row<LibraryManifestEntry> | undefined {
    if (index < 0 || index >= rows.length) return undefined;

    return rows[index];
  }

  function getVirtualizerResetKey(): string {
    return `${model.activeVendor}:${model.activeType}:${tableRows.length}`;
  }

  function scheduleVirtualizerReset(resetKey: string): void {
    const resetId = ++virtualizerResetId;

    void resetVirtualizerAfterLayout({
      viewport: scrollViewportRef,
      virtualizer: $rowVirtualizer,
      resetId,
      resetKey,
      currentResetId: () => virtualizerResetId,
      currentResetKey: getVirtualizerResetKey,
    });
  }
</script>

<section
  class="relative flex h-full min-h-0 flex-col gap-4 overflow-hidden"
  aria-busy={model.isBusy}
>
  {#if model.errorMessage}
    <Alert variant="destructive" class="shrink-0">
      <AlertTitle>Error</AlertTitle>
      <AlertDescription>{model.errorMessage}</AlertDescription>
    </Alert>
  {/if}

  <Tabs
    class="flex min-h-0 flex-1 flex-col overflow-hidden"
    value={model.activeVendor}
    onValueChange={model.handleVendorChange}
  >
    <TabsList class="flex shrink-0 flex-wrap">
      {#each vendorOptions as vendor (vendor.value)}
        <TabsTrigger value={vendor.value}>{vendor.label}</TabsTrigger>
      {/each}
    </TabsList>

    {#each vendorOptions as vendor (vendor.value)}
      <TabsContent value={vendor.value} class="flex min-h-0 flex-1 flex-col gap-4">
        {#if vendor.value === model.activeVendor}
          <ToggleGroup
            type="single"
            spacing={0}
            variant="outline"
            class="shrink-0 flex-wrap"
            value={model.activeType}
            onValueChange={model.handleTypeChange}
          >
            {#each typeOptionsByVendor[vendor.value] as type (type.value)}
              <ToggleGroupItem value={type.value}>{type.label}</ToggleGroupItem>
            {/each}
          </ToggleGroup>

          <ScrollArea
            bind:viewportRef={scrollViewportRef}
            orientation="both"
            class="min-h-0 flex-1"
          >
            <Table class="table-fixed">
              <TableHeader class="sticky top-0 z-10 bg-background">
                {#each table.getHeaderGroups() as headerGroup (headerGroup.id)}
                  <TableRow>
                    {#each headerGroup.headers as header (header.id)}
                      <TableHead class={getColumnClass(header.column.id)}>
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
                {#if model.emptyMessage}
                  <TableRow>
                    <TableCell
                      colspan={COLUMN_COUNT}
                      class="h-24 text-center text-muted-foreground"
                    >
                      {model.emptyMessage}
                    </TableCell>
                  </TableRow>
                {:else}
                  {#if topVirtualPadding > 0}
                    <TableRow aria-hidden="true">
                      <TableCell
                        colspan={COLUMN_COUNT}
                        style="height: {topVirtualPadding}px; padding: 0; border: 0;"
                      />
                    </TableRow>
                  {/if}

                  {#each virtualRows as virtualRow (virtualRow.key)}
                    {@const row = getRowByIndex(tableRows, virtualRow.index)}

                    {#if row}
                      <TableRow>
                        {#each row.getVisibleCells() as cell (cell.id)}
                          <TableCell class={getColumnClass(cell.column.id)}>
                            <FlexRender
                              content={cell.column.columnDef.cell}
                              context={cell.getContext()}
                            />
                          </TableCell>
                        {/each}
                      </TableRow>
                    {/if}
                  {/each}

                  {#if bottomVirtualPadding > 0}
                    <TableRow aria-hidden="true">
                      <TableCell
                        colspan={COLUMN_COUNT}
                        style="height: {bottomVirtualPadding}px; padding: 0; border: 0;"
                      />
                    </TableRow>
                  {/if}
                {/if}
              </TableBody>
            </Table>
          </ScrollArea>
        {/if}
      </TabsContent>
    {/each}
  </Tabs>

  <div class="absolute top-4 right-2">
    <ManifestRefreshButton
      refreshing={model.refreshing}
      disabled={model.isBusy}
      onRefresh={model.refreshManifest}
    />
  </div>
</section>
