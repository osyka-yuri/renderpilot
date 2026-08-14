import {
  columnVisibilityFeature,
  createCoreRowModel,
  createSortedRowModel,
  rowSortingFeature,
  tableFeatures,
} from '@tanstack/table-core';

export const coreTableFeatures = tableFeatures({
  columnVisibilityFeature,
  coreRowModel: createCoreRowModel(),
});

export const sortableTableFeatures = tableFeatures({
  columnVisibilityFeature,
  coreRowModel: createCoreRowModel(),
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
});

export type CoreTableFeatures = typeof coreTableFeatures;
export type SortableTableFeatures = typeof sortableTableFeatures;
