import type { Component, ComponentProps } from 'svelte';
import type { ColumnDef } from '@tanstack/table-core';
import { renderComponent } from '@shared/ui';
import { formatBytes } from '@shared/format';
import { t } from '@shared/i18n';
import type { LibraryManifestEntry } from '@entities/library';
import { formatSignedDate } from '../model/libraries-page-model';
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
  downloadedEntryIds: LibrariesPageModel['downloadedEntryIds'],
  onDownload: (entryId: string) => Promise<boolean>,
  onDelete: (entryId: string) => Promise<boolean>,
  getShowFileName: () => boolean,
): ColumnDef<LibraryManifestEntry>[] {
  return [
    {
      id: 'version',
      accessorFn: (row) => row.version.sort_key,
      header: ({ column }) =>
        renderTableCell(SortHeader, { label: t('libraries.column.version'), column }),
      cell: ({ row }) =>
        renderTableCell(LibraryVersionCell, {
          entry: row.original,
          showFileName: getShowFileName(),
        }),
    },
    {
      id: 'hash',
      header: () => t('libraries.column.hash'),
      enableSorting: false,
      cell: ({ row }) => renderTableCell(LibraryHashCell, { entry: row.original }),
    },
    {
      id: 'signed',
      accessorFn: (row) => (row.signature.status === 'signed' ? row.signature.signed_at : ''),
      header: ({ column }) =>
        renderTableCell(SortHeader, { label: t('libraries.column.signed'), column }),
      cell: ({ row }) => formatSignedDate(row.original.signature),
    },
    {
      id: 'size',
      accessorFn: (row) => row.files.dll.size_bytes,
      header: ({ column }) =>
        renderTableCell(SortHeader, { label: t('libraries.column.size'), column }),
      cell: ({ row }) => formatBytes(row.original.files.dll.size_bytes),
    },
    {
      id: 'actions',
      header: () => t('libraries.column.actions'),
      enableSorting: false,
      cell: ({ row }) =>
        renderTableCell(LibraryActionsCell, {
          entry: row.original,
          pendingActions,
          downloadedEntryIds,
          onDownload,
          onDelete,
        }),
    },
  ];
}
