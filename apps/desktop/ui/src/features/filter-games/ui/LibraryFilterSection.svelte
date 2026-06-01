<script lang="ts">
  import { ToggleGroup, ToggleGroupItem } from '@shared/ui';
  import { t } from '@shared/i18n';
  import {
    mergeVendorDraftLibraries,
    selectedLibrariesForVendor,
    type GroupedLibraryFilterOptions,
  } from '../model/library-filter-options';

  type Props = {
    groupedOptions?: readonly GroupedLibraryFilterOptions[];
    draftLibraries?: readonly string[];
    onLibrariesChange?: (libraries: readonly string[]) => void;
  };

  type VendorKey = GroupedLibraryFilterOptions['vendorKey'];
  type VendorOptions = GroupedLibraryFilterOptions['options'];

  const LIBRARIES_HEADING_ID = 'library-filters-heading';

  const EMPTY_GROUPED_OPTIONS = [] as const satisfies readonly GroupedLibraryFilterOptions[];
  const EMPTY_DRAFT_LIBRARIES = [] as const satisfies readonly string[];

  let {
    groupedOptions = EMPTY_GROUPED_OPTIONS,
    draftLibraries = EMPTY_DRAFT_LIBRARIES,
    onLibrariesChange,
  }: Props = $props();

  function getVendorHeadingId(vendorKey: VendorKey, index: number): string {
    const normalizedVendorKey = vendorKey.trim().replace(/[^a-zA-Z0-9_-]/g, '-');

    return `${LIBRARIES_HEADING_ID}-${index}-${normalizedVendorKey || 'unknown'}`;
  }

  function getSelectedLibraries(vendorOptions: VendorOptions): string[] {
    return selectedLibrariesForVendor(draftLibraries, vendorOptions);
  }

  function areSameLibraries(
    currentLibraries: readonly string[],
    nextLibraries: readonly string[],
  ): boolean {
    return (
      currentLibraries.length === nextLibraries.length &&
      currentLibraries.every((library, index) => library === nextLibraries[index])
    );
  }

  function handleLibraryGroupChange(vendorOptions: VendorOptions, nextValue: string[]): void {
    if (!onLibrariesChange) {
      return;
    }

    const nextLibraries = mergeVendorDraftLibraries(draftLibraries, vendorOptions, nextValue);

    if (areSameLibraries(draftLibraries, nextLibraries)) {
      return;
    }

    onLibrariesChange(nextLibraries);
  }
</script>

<section class="grid gap-3" aria-labelledby={LIBRARIES_HEADING_ID}>
  <h3 id={LIBRARIES_HEADING_ID} class="text-sm font-medium">
    {t('filters.libraries.title')}
  </h3>

  {#if groupedOptions.length > 0}
    <div class="grid gap-4">
      {#each groupedOptions as vendorGroup, index (vendorGroup.vendorKey)}
        {@const vendorHeadingId = getVendorHeadingId(vendorGroup.vendorKey, index)}

        <div class="grid gap-2">
          <h4 id={vendorHeadingId} class="text-xs font-medium text-muted-foreground">
            {vendorGroup.vendorLabel}
          </h4>

          <ToggleGroup
            type="multiple"
            variant="outline"
            class="w-full"
            aria-labelledby={vendorHeadingId}
            value={getSelectedLibraries(vendorGroup.options)}
            onValueChange={(nextValue: string[]) => {
              handleLibraryGroupChange(vendorGroup.options, nextValue);
            }}
          >
            {#each vendorGroup.options as option (option.value)}
              <ToggleGroupItem value={option.value} class="flex-1" size="sm">
                {option.label}
              </ToggleGroupItem>
            {/each}
          </ToggleGroup>
        </div>
      {/each}
    </div>
  {:else}
    <p class="text-sm text-muted-foreground">
      {t('filters.libraries.empty')}
    </p>
  {/if}
</section>
