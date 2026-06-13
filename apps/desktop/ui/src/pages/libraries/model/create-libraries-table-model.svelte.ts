import { createVirtualizer } from '@tanstack/svelte-virtual';
import { untrack } from 'svelte';
import { get } from 'svelte/store';
import {
  getCoreRowModel,
  getSortedRowModel,
  type ColumnDef,
  type Row,
  type SortingState,
} from '@tanstack/table-core';
import { createSvelteTable } from '@shared/ui';
import type { LibraryManifestEntry } from '@entities/library';
import { resetVirtualizerAfterLayout } from '../ui/virtualizer-helpers';

const DEFAULT_SORTING: SortingState = [{ id: 'version', desc: true }];
const ROW_ESTIMATE_SIZE_SINGLE = 40;
const ROW_ESTIMATE_SIZE_MULTI = 52;
const ROW_OVERSCAN = 12;

const COLUMN_IDS = ['version', 'hash', 'signed', 'size', 'actions'] as const;
export const COLUMN_COUNT = COLUMN_IDS.length;

const COLUMN_CLASS_BY_ID = {
  version: 'w-56',
  hash: 'w-64',
  signed: 'w-40',
  size: 'w-24',
  actions: 'w-24 text-end',
} satisfies Readonly<Record<(typeof COLUMN_IDS)[number], string>>;

export function getColumnClass(columnId: string): string {
  return COLUMN_CLASS_BY_ID[columnId as keyof typeof COLUMN_CLASS_BY_ID];
}

type LibrariesTableModelProps = {
  getEntries: () => LibraryManifestEntry[];
  getColumns: () => ColumnDef<LibraryManifestEntry>[];
  getActiveVendor: () => string | undefined;
  getActiveType: () => string | undefined;
  getShowFileName: () => boolean;
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
    return `${props.getActiveVendor()}:${props.getActiveType()}:${tableRows.length}`;
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

  const table = $derived(
    createSvelteTable({
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
      getCoreRowModel: getCoreRowModel(),
      getSortedRowModel: getSortedRowModel(),
    }),
  );

  const tableRows = $derived(table.getRowModel().rows);

  const rowVirtualizer = $derived.by(() => {
    const scrollElement = scrollViewportRef;
    const rows = tableRows;

    return createVirtualizer<HTMLElement, HTMLTableRowElement>({
      count: rows.length,
      getScrollElement: () => scrollElement,
      estimateSize: () =>
        props.getShowFileName() ? ROW_ESTIMATE_SIZE_MULTI : ROW_ESTIMATE_SIZE_SINGLE,
      overscan: ROW_OVERSCAN,
      getItemKey: (index) => getRowByIndex(rows, index)?.original.entry_id ?? index,
    });
  });

  function getRowByIndex(
    rows: Row<LibraryManifestEntry>[],
    index: number,
  ): Row<LibraryManifestEntry> | undefined {
    if (index < 0 || index >= rows.length) return undefined;
    return rows[index];
  }

  $effect(() => {
    const viewport = scrollViewportRef;
    const rowCount = tableRows.length;
    const resetKey = getVirtualizerResetKey();

    if (viewport === null || rowCount === 0) return;

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
