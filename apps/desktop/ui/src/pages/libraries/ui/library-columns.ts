import type { Component, ComponentProps } from 'svelte';
import type { ColumnDef } from '@tanstack/table-core';
import { renderComponent } from '@shared/ui';
import { formatBytes } from '@shared/format';
import { t } from '@shared/i18n';
import {
  compareReleaseVersions,
  formatSignedDate,
  type LibraryPackageRow,
} from '../model/libraries-page-model';
import type { LibrariesPageModel } from '../model/create-libraries-page-model.svelte';
import LibraryActionsCell from './LibraryActionsCell.svelte';
import LibraryHashCell from './LibraryHashCell.svelte';
import LibraryLegalCell from './LibraryLegalCell.svelte';
import LibraryVersionCell from './LibraryVersionCell.svelte';
import SortHeader from './SortHeader.svelte';

const LIBRARY_COLUMN_LAYOUT = [
  { id: 'version', className: 'w-56' },
  { id: 'hash', className: 'w-64' },
  { id: 'signed', className: 'w-36 text-center' },
  { id: 'size', className: 'w-24 text-center' },
  { id: 'documents', className: 'w-24 text-center' },
  { id: 'actions', className: 'w-24 text-center' },
] as const;

type LibraryColumnId = (typeof LIBRARY_COLUMN_LAYOUT)[number]['id'];

export const LIBRARY_COLUMN_COUNT = LIBRARY_COLUMN_LAYOUT.length;

export function getLibraryColumnClass(columnId: string): string {
  return LIBRARY_COLUMN_LAYOUT.find((column) => column.id === columnId)?.className ?? '';
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function renderTableCell<TComponent extends Component<any, any, any>>(
  component: TComponent,
  props: ComponentProps<TComponent>,
): ReturnType<typeof renderComponent> {
  return renderComponent(component, props);
}

/**
 * Builds the column defs once per page. Every input is a stable reference
 * (reactive containers + model callbacks); per-row state is derived inside the
 * cell components, so the columns never change identity — recreating them
 * would rebuild the table rows and reset the scroll position.
 */
export function createLibraryColumns(
  pendingActions: LibrariesPageModel['pendingActions'],
  actionsDisabled: () => boolean,
  onDownload: (packageId: string) => Promise<boolean>,
  onDelete: (packageId: string) => Promise<boolean>,
  showPackageDisplayName: () => boolean,
  onShowLegalDocuments: (row: LibraryPackageRow) => void,
): ColumnDef<LibraryPackageRow>[] {
  const columnsById = {
    version: {
      id: 'version',
      accessorFn: (row) => row.release.version,
      sortingFn: (left, right) =>
        compareReleaseVersions(left.original.release.version, right.original.release.version),
      header: ({ column }) =>
        renderTableCell(SortHeader, { label: t('libraries.column.version'), column }),
      cell: ({ row }) =>
        renderTableCell(LibraryVersionCell, {
          row: row.original,
          showPackageDisplayName,
        }),
    },
    hash: {
      id: 'hash',
      header: () => t('libraries.column.hash'),
      enableSorting: false,
      cell: ({ row }) => renderTableCell(LibraryHashCell, { row: row.original }),
    },
    signed: {
      id: 'signed',
      accessorFn: (row) =>
        row.primary_signature.status === 'signed' ? row.primary_signature.signed_at : '',
      header: ({ column }) =>
        renderTableCell(SortHeader, {
          label: t('libraries.column.signed'),
          column,
          class: 'w-full justify-center',
        }),
      cell: ({ row }) => formatSignedDate(row.original.primary_signature),
    },
    size: {
      id: 'size',
      accessorFn: (row) => row.size_bytes,
      header: ({ column }) =>
        renderTableCell(SortHeader, {
          label: t('libraries.column.size'),
          column,
          class: 'w-full justify-center',
        }),
      cell: ({ row }) => formatBytes(row.original.size_bytes),
    },
    documents: {
      id: 'documents',
      header: () => t('libraries.column.documents'),
      enableSorting: false,
      cell: ({ row }) =>
        renderTableCell(LibraryLegalCell, {
          row: row.original,
          onOpen: onShowLegalDocuments,
        }),
    },
    actions: {
      id: 'actions',
      header: () => t('libraries.column.actions'),
      enableSorting: false,
      cell: ({ row }) =>
        renderTableCell(LibraryActionsCell, {
          row: row.original,
          pendingActions,
          actionsDisabled,
          onDownload,
          onDelete,
        }),
    },
  } satisfies Record<LibraryColumnId, ColumnDef<LibraryPackageRow>>;

  return LIBRARY_COLUMN_LAYOUT.map(({ id }) => columnsById[id]);
}
