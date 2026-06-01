import type { Component, ComponentProps } from 'svelte';
import type { ColumnDef } from '@tanstack/table-core';
import { renderComponent } from '@shared/ui';
import { formatBytes } from '@shared/format';
import { t } from '@shared/i18n';
import type { LibraryManifestEntry } from '@entities/library';
import { formatSignedDate, formatVersionLabel } from '../model/libraries-page-model';
import type { LibrariesPageModel } from '../model/create-libraries-page-model.svelte';
import LibraryActionsCell from './LibraryActionsCell.svelte';
import LibraryHashCell from './LibraryHashCell.svelte';
import SortHeader from './SortHeader.svelte';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function renderTableCell<TComponent extends Component<any, any, any>>(
  component: TComponent,
  props: ComponentProps<TComponent>,
): ReturnType<typeof renderComponent> {
  return renderComponent(component, props);
}

export function createLibraryColumns(
  pendingEntryAction: LibrariesPageModel['pendingEntryAction'],
  currentDownloadedEntryIds: ReadonlySet<string>,
  onDownload: (entryId: string) => Promise<void>,
  onDelete: (entryId: string) => Promise<void>,
): ColumnDef<LibraryManifestEntry>[] {
  return [
    {
      id: 'version',
      accessorFn: (row) => row.version.sort_key,
      header: ({ column }) =>
        renderTableCell(SortHeader, { label: t('libraries.column.version'), column }),
      cell: ({ row }) => formatVersionLabel(row.original),
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
          pendingEntryAction,
          isDownloaded: currentDownloadedEntryIds.has(row.original.entry_id),
          onDownload,
          onDelete,
        }),
    },
  ];
}
