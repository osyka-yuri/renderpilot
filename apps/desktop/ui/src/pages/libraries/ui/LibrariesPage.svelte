<script lang="ts">
  import { untrack } from 'svelte';
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
  } from '@shared/ui';
  import { t } from '@shared/i18n';

  import {
    findPackageRow,
    typeOptionsByVendor,
    vendorOptions,
  } from '../model/libraries-page-model';
  import { createLibrariesPageModel } from '../model/create-libraries-page-model.svelte';

  import {
    createLibraryColumns,
    LIBRARY_COLUMN_COUNT,
    getLibraryColumnClass,
  } from './library-columns';
  import { createLibrariesTableModel } from '../model/create-libraries-table-model.svelte';
  import { getBottomVirtualPadding, getTopVirtualPadding } from './virtualizer-helpers';
  import LibrariesBulkDownloadButton from './LibrariesBulkDownloadButton.svelte';
  import LibraryLegalDocumentsSheet from './LibraryLegalDocumentsSheet.svelte';

  type Props = {
    refreshKey?: number;
  };

  const { refreshKey = 0 }: Props = $props();

  const model = createLibrariesPageModel();
  let legalDocumentsPackageId = $state<string | null>(null);
  const legalDocumentsRow = $derived(findPackageRow(model.packages, legalDocumentsPackageId));

  $effect(() => {
    const requestedRefreshKey = refreshKey;
    untrack(() => {
      model.requestCatalogRefresh(requestedRefreshKey);
    });
  });

  // Static column defs over stable reactive containers — see createLibraryColumns.
  const columns = createLibraryColumns(
    model.pendingActions,
    () => model.loading || model.refreshing || model.bulkDownloading,
    model.handleDownload,
    model.handleDelete,
    () => model.showPackageDisplayName,
    (row) => {
      legalDocumentsPackageId = row.package_id;
    },
  );

  const tableModel = createLibrariesTableModel({
    getEntries: () => model.filteredPackages,
    getColumns: () => columns,
    getActiveVendor: () => model.activeVendor,
    getActiveType: () => model.activeType,
    getShowPackageDisplayName: () => model.showPackageDisplayName,
  });

  const rowVirtualizer = $derived(tableModel.rowVirtualizer);
  const virtualRows = $derived($rowVirtualizer.getVirtualItems());
  const topVirtualPadding = $derived(getTopVirtualPadding(virtualRows));
  const bottomVirtualPadding = $derived(
    getBottomVirtualPadding(virtualRows, $rowVirtualizer.getTotalSize()),
  );

  $effect(() => {
    // The initial load updates model state. Tracking it here would rerun this
    // effect and dispose the still-mounted page model.
    untrack(() => {
      void model.start();
    });

    return () => {
      model.dispose();
      tableModel.dispose();
    };
  });

  $effect(() => {
    if (legalDocumentsPackageId !== null && !model.loading && legalDocumentsRow === null) {
      legalDocumentsPackageId = null;
    }
  });
</script>

<section
  class="relative flex h-full min-h-0 flex-col gap-4 overflow-hidden"
  aria-busy={model.isBusy}
>
  {#if model.errorMessage}
    <Alert variant="destructive" class="shrink-0">
      <AlertTitle>{t('libraries.error')}</AlertTitle>
      <AlertDescription>{model.errorMessage}</AlertDescription>
    </Alert>
  {/if}

  <Tabs
    class="flex min-h-0 flex-1 flex-col overflow-hidden"
    value={model.activeVendor}
    onValueChange={model.handleVendorChange}
  >
    <div class="flex shrink-0 flex-wrap items-center justify-between gap-3">
      <TabsList class="flex flex-wrap">
        {#each vendorOptions as vendor (vendor.value)}
          <TabsTrigger value={vendor.value}>{vendor.label}</TabsTrigger>
        {/each}
      </TabsList>

      <div class="flex flex-wrap items-center gap-2">
        <LibrariesBulkDownloadButton {model} />
      </div>
    </div>

    {#each vendorOptions as vendor (vendor.value)}
      <TabsContent value={vendor.value} class="flex min-h-0 flex-1 flex-col gap-4">
        {#if vendor.value === model.activeVendor}
          <ToggleGroup
            type="single"
            spacing={0}
            variant="outline"
            class="shrink-0 flex-wrap"
            bind:value={model.activeType}
          >
            {#each typeOptionsByVendor[vendor.value] as type (type.value)}
              <ToggleGroupItem value={type.value}>{type.label}</ToggleGroupItem>
            {/each}
          </ToggleGroup>

          <ScrollArea
            bind:viewportRef={tableModel.scrollViewportRef}
            orientation="both"
            class="min-h-0 flex-1"
          >
            <Table class="table-fixed">
              <TableHeader class="sticky top-0 z-10 bg-background">
                {#each tableModel.table.getHeaderGroups() as headerGroup (headerGroup.id)}
                  <TableRow>
                    {#each headerGroup.headers as header (header.id)}
                      <TableHead class={getLibraryColumnClass(header.column.id)}>
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
                      colspan={LIBRARY_COLUMN_COUNT}
                      class="h-24 text-center text-muted-foreground"
                    >
                      {model.emptyMessage}
                    </TableCell>
                  </TableRow>
                {:else}
                  {#if topVirtualPadding > 0}
                    <TableRow aria-hidden="true">
                      <TableCell
                        colspan={LIBRARY_COLUMN_COUNT}
                        style="height: {topVirtualPadding}px; padding: 0; border: 0;"
                      />
                    </TableRow>
                  {/if}

                  {#each virtualRows as virtualRow (virtualRow.key)}
                    {@const row = tableModel.getRowByIndex(tableModel.tableRows, virtualRow.index)}

                    {#if row}
                      <TableRow>
                        {#each row.getVisibleCells() as cell (cell.id)}
                          <TableCell class={getLibraryColumnClass(cell.column.id)}>
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
                        colspan={LIBRARY_COLUMN_COUNT}
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
</section>

<LibraryLegalDocumentsSheet
  row={legalDocumentsRow}
  onClose={() => {
    legalDocumentsPackageId = null;
  }}
/>
