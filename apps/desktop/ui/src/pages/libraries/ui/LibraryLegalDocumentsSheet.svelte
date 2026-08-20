<script lang="ts">
  import ExternalLinkIcon from '@lucide/svelte/icons/external-link';
  import FileTextIcon from '@lucide/svelte/icons/file-text';
  import { openExternal } from '@shared/api';
  import { t } from '@shared/i18n';
  import { publishErrorNotification } from '@shared/notifications';
  import {
    Button,
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemGroup,
    ItemMedia,
    ItemTitle,
    ScrollArea,
    Sheet,
    SheetContent,
    SheetDescription,
    SheetHeader,
    SheetTitle,
  } from '@shared/ui';

  import type { LibraryLegalDocumentLink } from '@entities/library';
  import type { LibraryPackageRow } from '../model/libraries-page-model';
  import { formatVersionLabel } from '../model/libraries-page-model';

  type Props = {
    row: LibraryPackageRow | null;
    onClose: () => void;
  };

  const { row, onClose }: Props = $props();
  let openingDocumentId = $state<string | null>(null);

  function formatLabel(document: LibraryLegalDocumentLink): string {
    return t(
      document.format === 'pdf'
        ? 'libraries.documents.formatPdf'
        : 'libraries.documents.formatText',
    );
  }

  async function openDocument(document: LibraryLegalDocumentLink): Promise<void> {
    if (openingDocumentId !== null) {
      return;
    }
    openingDocumentId = document.legal_document_id;
    try {
      await openExternal(document.content_url);
    } catch {
      publishErrorNotification(t('libraries.documents.openFailed'));
    } finally {
      openingDocumentId = null;
    }
  }
</script>

<Sheet
  open={row !== null}
  onOpenChange={(open: boolean) => {
    if (!open) {
      onClose();
    }
  }}
>
  {#if row}
    <SheetContent closeLabel={t('common.close')} class="w-full gap-0 sm:max-w-lg">
      <SheetHeader class="border-b p-6 pe-12">
        <SheetTitle>{t('libraries.documents.title')}</SheetTitle>
        <SheetDescription>
          {t('libraries.documents.description', {
            name: row.display_name,
            version: formatVersionLabel(row),
          })}
        </SheetDescription>
      </SheetHeader>

      <ScrollArea class="min-h-0 flex-1">
        <ItemGroup class="gap-3 p-6">
          {#each row.legal_documents as document (document.legal_document_id)}
            <Item variant="outline" class="flex-nowrap">
              <ItemMedia variant="icon">
                <FileTextIcon class="size-4" aria-hidden="true" />
              </ItemMedia>
              <ItemContent class="min-w-0">
                <ItemTitle>{document.title}</ItemTitle>
                <ItemDescription class="truncate">
                  {formatLabel(document)} · {document.file_name}
                </ItemDescription>
              </ItemContent>
              <ItemActions>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={openingDocumentId !== null}
                  onclick={() => void openDocument(document)}
                >
                  <ExternalLinkIcon class="size-4" aria-hidden="true" />
                  {t('libraries.documents.open')}
                </Button>
              </ItemActions>
            </Item>
          {/each}
        </ItemGroup>
      </ScrollArea>
    </SheetContent>
  {/if}
</Sheet>
