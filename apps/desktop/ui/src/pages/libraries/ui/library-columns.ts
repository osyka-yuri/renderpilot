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
import LibraryVersionCell from './LibraryVersionCell.svelte';
import SortHeader from './SortHeader.svelte';

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
): ColumnDef<LibraryPackageRow>[] {
  return [
    {
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
    {
      id: 'hash',
      header: () => t('libraries.column.hash'),
      enableSorting: false,
      cell: ({ row }) => renderTableCell(LibraryHashCell, { row: row.original }),
    },
    {
      id: 'signed',
      accessorFn: (row) =>
        row.primary_signature.status === 'signed' ? row.primary_signature.signed_at : '',
      header: ({ column }) =>
        renderTableCell(SortHeader, { label: t('libraries.column.signed'), column }),
      cell: ({ row }) => formatSignedDate(row.original.primary_signature),
    },
    {
      id: 'size',
      accessorFn: (row) => row.size_bytes,
      header: ({ column }) =>
        renderTableCell(SortHeader, { label: t('libraries.column.size'), column }),
      cell: ({ row }) => formatBytes(row.original.size_bytes),
    },
    {
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
  ];
}
