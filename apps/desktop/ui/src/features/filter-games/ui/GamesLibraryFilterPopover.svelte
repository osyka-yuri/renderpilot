<script lang="ts">
  import {
    mergeVendorDraftLibraries,
    selectedLibrariesForVendor,
    type GroupedLibraryFilterOptions,
  } from '../model/library-filter-options';

  import { Button, Separator, ToggleGroup, ToggleGroupItem } from '@shared/ui';

  type LibraryValue = string;
  type LibrariesChangeHandler = (libraries: readonly LibraryValue[]) => void;
  type VoidHandler = () => void;

  type Props = {
    groupedLibraryFilterOptions?: readonly GroupedLibraryFilterOptions[];
    draftLibraries?: readonly LibraryValue[];
    onDraftLibrariesChange?: LibrariesChangeHandler;
    onCancel?: VoidHandler;
    onApply?: VoidHandler;
  };

  const LIBRARIES_LABEL = 'Libraries';
  const EMPTY_LIBRARIES_LABEL = 'No libraries detected';

  const {
    groupedLibraryFilterOptions = [],
    draftLibraries = [],
    onDraftLibrariesChange,
    onCancel,
    onApply,
  }: Props = $props();

  function handleGroupValueChange(
    vendorOptions: { value: string }[],
    nextValue: string[],
  ): void {
    onDraftLibrariesChange?.(mergeVendorDraftLibraries(draftLibraries, vendorOptions, nextValue));
  }

  function groupValue(vendorOptions: { value: string }[]): string[] {
    return selectedLibrariesForVendor(draftLibraries, vendorOptions);
  }
</script>

<div class="grid gap-3">
  <div class="grid gap-1 text-sm">
    <h4 class="font-medium">{LIBRARIES_LABEL}</h4>
  </div>

  {#if groupedLibraryFilterOptions.length > 0}
    <div class="grid gap-4">
      {#each groupedLibraryFilterOptions as vendorGroup (vendorGroup.vendorKey)}
        <div class="grid gap-2">
          <h5 class="text-xs font-medium text-muted-foreground">{vendorGroup.vendorLabel}</h5>

          <ToggleGroup
            type="multiple"
            variant="outline"
            class="w-full"
            value={groupValue(vendorGroup.options)}
            onValueChange={(next: string[]) => {
              handleGroupValueChange(vendorGroup.options, next);
            }}
          >
            {#each vendorGroup.options as option (option.value)}
              <ToggleGroupItem value={option.value} class="flex-1">
                {option.label}
              </ToggleGroupItem>
            {/each}
          </ToggleGroup>
        </div>
      {/each}
    </div>
  {:else}
    <span class="text-sm text-muted-foreground">{EMPTY_LIBRARIES_LABEL}</span>
  {/if}

  <Separator />

  <div class="flex items-center justify-end gap-2">
    <Button variant="secondary" size="sm" onclick={onCancel}>Cancel</Button>
    <Button variant="default" size="sm" onclick={onApply}>Apply</Button>
  </div>
</div>
