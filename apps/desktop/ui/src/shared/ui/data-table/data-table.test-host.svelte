<script lang="ts">
  import type { ColumnDef, SortingState } from '@tanstack/table-core';
  import { untrack } from 'svelte';

  import { createSvelteTable } from './data-table.svelte.js';
  import { sortableTableFeatures } from './table-features.js';

  type TestRow = {
    id: string;
    label: string;
    value: number;
  };

  type Props = {
    initialSorting?: SortingState;
    onSortingChange?: (sorting: SortingState) => void;
  };

  const { initialSorting = [], onSortingChange }: Props = $props();

  let data = $state<TestRow[]>([
    { id: 'two', label: 'Two', value: 2 },
    { id: 'one', label: 'One', value: 1 },
  ]);
  let columns = $state<ColumnDef<typeof sortableTableFeatures, TestRow>[]>(baseColumns());
  let sorting = $state<SortingState>(untrack(() => initialSorting));

  const table = createSvelteTable(sortableTableFeatures, {
    get columns() {
      return columns;
    },
    get data() {
      return data;
    },
    getRowId: (row) => row.id,
    state: {
      get sorting() {
        return sorting;
      },
    },
    onSortingChange: (updater) => {
      sorting = typeof updater === 'function' ? updater(sorting) : updater;
      onSortingChange?.(sorting);
    },
  });

  const renderedRows = $derived(
    table
      .getRowModel()
      .rows.map((row) => row.original.id)
      .join(','),
  );
  const visibleColumnIds = $derived(
    table
      .getRowModel()
      .rows[0]?.getVisibleCells()
      .map((cell) => cell.column.id)
      .join(',') ?? '',
  );
  const currentIdSortState = $derived(table.getColumn('id')?.getIsSorted() ?? false);
  const renderedIdSortState = $derived(currentIdSortState === false ? 'none' : currentIdSortState);

  export function addDataRow(): void {
    data = [...data, { id: 'three', label: 'Three', value: 3 }];
  }

  export function addColumn(): void {
    columns = [
      ...columns,
      {
        id: 'label',
        accessorKey: 'label',
      },
    ];
  }

  export function getRowIds(): string[] {
    return table.getRowModel().rows.map((row) => row.original.id);
  }

  export function getTable(): typeof table {
    return table;
  }

  export function getVisibleCellCount(): number {
    return table.getRowModel().rows[0]?.getVisibleCells().length ?? 0;
  }

  export function hideIdColumn(): void {
    table.getColumn('id')?.toggleVisibility(false);
  }

  export function sortByValue(): void {
    table.getColumn('value')?.toggleSorting(false);
  }

  export function toggleValueSorting(): void {
    table.getColumn('value')?.toggleSorting();
  }

  export function toggleIdSorting(): void {
    table.getColumn('id')?.toggleSorting();
  }

  export function getSorting(): SortingState {
    return sorting;
  }

  export function getIdSortState(): false | 'asc' | 'desc' {
    return table.getColumn('id')?.getIsSorted() ?? false;
  }

  function baseColumns(): ColumnDef<typeof sortableTableFeatures, TestRow>[] {
    return [
      { id: 'id', accessorKey: 'id' },
      { id: 'value', accessorKey: 'value' },
    ];
  }
</script>

<output data-id-sort={renderedIdSortState}>{renderedRows}:{visibleColumnIds}</output>
