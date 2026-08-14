import { createVirtualizer } from '@tanstack/svelte-virtual';
import { untrack } from 'svelte';
import { get } from 'svelte/store';
import { type ColumnDef, type Row, type SortingState } from '@tanstack/table-core';
import { createSvelteTable, sortableTableFeatures } from '@shared/ui';
import type { LibraryPackageRow } from './libraries-page-model';
import { resetVirtualizerAfterLayout } from '../ui/virtualizer-helpers';

const DEFAULT_SORTING: SortingState = [{ id: 'version', desc: true }];
const ROW_ESTIMATE_SIZE_SINGLE_LINE = 40;
const ROW_ESTIMATE_SIZE_WITH_NAME = 52;
const ROW_OVERSCAN = 12;

type LibrariesTableModelProps = {
  getEntries: () => LibraryPackageRow[];
  getColumns: () => ColumnDef<typeof sortableTableFeatures, LibraryPackageRow>[];
  getActiveVendor: () => string | undefined;
  getActiveType: () => string | undefined;
  getShowPackageDisplayName: () => boolean;
};

/**
 * Encapsulates the TanStack Table and Virtualizer state for the Libraries grid.
 * Provides a reactive facade over sorting, virtualization, and layout properties.
 */
export function createLibrariesTableModel(props: LibrariesTableModelProps) {
  let sorting = $state<SortingState>([...DEFAULT_SORTING]);
  let scrollViewportRef = $state<HTMLElement | null>(null);
  let virtualizerResetId = 0;

  function getVirtualizerResetKey(): string {
    return `${props.getActiveVendor()}:${props.getActiveType()}:${props.getShowPackageDisplayName()}:${tableRows.length}`;
  }

  function scheduleVirtualizerReset(resetKey: string): void {
    const resetId = ++virtualizerResetId;
    void resetVirtualizerAfterLayout({
      viewport: scrollViewportRef,
      virtualizer: untrack(() => get(rowVirtualizer)),
      resetId,
      resetKey,
      currentResetId: () => virtualizerResetId,
      currentResetKey: getVirtualizerResetKey,
    });
  }

  const table = createSvelteTable(sortableTableFeatures, {
    get data() {
      return props.getEntries();
    },
    get columns() {
      return props.getColumns();
    },
    state: {
      get sorting() {
        return sorting;
      },
    },
    onSortingChange: (updater) => {
      sorting = typeof updater === 'function' ? updater(sorting) : updater;
      scheduleVirtualizerReset(getVirtualizerResetKey());
    },
  });

  const tableRows = $derived(table.getRowModel().rows);

  const rowVirtualizer = $derived.by(() => {
    const scrollElement = scrollViewportRef;
    const rows = tableRows;
    const showPackageDisplayName = props.getShowPackageDisplayName();

    return createVirtualizer<HTMLElement, HTMLTableRowElement>({
      count: rows.length,
      getScrollElement: () => scrollElement,
      estimateSize: () =>
        showPackageDisplayName ? ROW_ESTIMATE_SIZE_WITH_NAME : ROW_ESTIMATE_SIZE_SINGLE_LINE,
      overscan: ROW_OVERSCAN,
      getItemKey: (index) => getRowByIndex(rows, index)?.original.package_id ?? index,
    });
  });

  function getRowByIndex(
    rows: Row<typeof sortableTableFeatures, LibraryPackageRow>[],
    index: number,
  ): Row<typeof sortableTableFeatures, LibraryPackageRow> | undefined {
    if (index < 0 || index >= rows.length) {
      return undefined;
    }
    return rows[index];
  }

  $effect(() => {
    const viewport = scrollViewportRef;
    const rowCount = tableRows.length;
    const resetKey = getVirtualizerResetKey();

    if (viewport === null || rowCount === 0) {
      return;
    }

    scheduleVirtualizerReset(resetKey);
  });

  return {
    get scrollViewportRef() {
      return scrollViewportRef;
    },
    set scrollViewportRef(value: HTMLElement | null) {
      scrollViewportRef = value;
    },
    get table() {
      return table;
    },
    get tableRows() {
      return tableRows;
    },
    get rowVirtualizer() {
      return rowVirtualizer;
    },
    getRowByIndex,
    dispose: () => {
      virtualizerResetId += 1;
    },
  };
}
