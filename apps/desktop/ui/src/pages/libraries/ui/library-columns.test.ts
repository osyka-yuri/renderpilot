import {
  constructTable,
  sortFn_alphanumeric,
  sortFn_basic,
  tableFeatures,
} from '@tanstack/table-core';
import { storeReactivityBindings } from '@tanstack/table-core/store-reactivity-bindings';
import { describe, expect, it } from 'vitest';

import { sortableTableFeatures } from '@shared/ui';

import { packagesOf, type PackageFixture } from '../model/library-package-test-fixtures';
import { createLibraryColumns } from './library-columns';

const tableFeaturesWithStore = tableFeatures({
  ...sortableTableFeatures,
  coreReactivityFeature: storeReactivityBindings(),
});

function rowsSortedBy(columnId: string, specs: readonly PackageFixture[]): string[] {
  const table = constructTable({
    features: tableFeaturesWithStore,
    data: packagesOf(specs),
    columns: createLibraryColumns(
      new Map(),
      () => false,
      () => Promise.resolve(true),
      () => Promise.resolve(true),
      () => false,
      () => undefined,
    ),
    state: {
      sorting: [{ id: columnId, desc: false }],
    },
  });

  return table.getRowModel().rows.map((row) => row.original.package_id);
}

describe('library table columns', () => {
  it('keeps release versions on the custom package-version comparator', () => {
    expect(
      rowsSortedBy('version', [
        { id: 'release', version: '1.0.0' },
        { id: 'preview', version: '1.0.0-preview.1' },
        { id: 'earlier', version: '0.9.9' },
      ]),
    ).toEqual(['earlier', 'preview', 'release']);
  });

  it('uses direct alphanumeric and numeric comparators for signed dates and sizes', () => {
    const columns = createLibraryColumns(
      new Map(),
      () => false,
      () => Promise.resolve(true),
      () => Promise.resolve(true),
      () => false,
      () => undefined,
    );

    expect(columns.find((column) => column.id === 'signed')?.sortFn).toBe(sortFn_alphanumeric);
    expect(columns.find((column) => column.id === 'size')?.sortFn).toBe(sortFn_basic);
    expect(
      rowsSortedBy('signed', [
        { id: 'later', signature: { status: 'signed', signed_at: '2026-10-01T00:00:00Z' } },
        { id: 'earlier', signature: { status: 'signed', signed_at: '2026-02-01T00:00:00Z' } },
      ]),
    ).toEqual(['earlier', 'later']);
    expect(
      rowsSortedBy('size', [
        { id: 'ten', sizeBytes: 10 },
        { id: 'two', sizeBytes: 2 },
      ]),
    ).toEqual(['two', 'ten']);
  });
});
