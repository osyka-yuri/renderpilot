<script lang="ts">
  import FunnelIcon from '@lucide/svelte/icons/funnel';
  import {
    Button,
    Dialog,
    DialogContent,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
    Separator,
    buttonVariants,
  } from '@shared/ui';
  import { type GroupedLibraryFilterOptions } from '../model/library-filter-options';
  import { type LauncherFilterOption } from '../model/launcher-filter-options';
  import LauncherFilterSection from './LauncherFilterSection.svelte';
  import LibraryFilterSection from './LibraryFilterSection.svelte';

  type Props = {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    hasFilterIndicator: boolean;
    filtersButtonLabel: string;
    groupedLibraryFilterOptions?: readonly GroupedLibraryFilterOptions[];
    draftLibraries?: readonly string[];
    onDraftLibrariesChange?: (libraries: readonly string[]) => void;
    launcherFilterOptions?: readonly LauncherFilterOption[];
    draftLaunchers?: readonly string[];
    onDraftLaunchersChange?: (launchers: readonly string[]) => void;
    draftLauncherOrder?: readonly string[];
    onDraftLauncherOrderChange?: (order: readonly string[]) => void;
    onCancel?: () => void;
    onApply?: () => void;
  };

  const DIALOG_TITLE = 'Filters';
  const EMPTY_ARRAY = [] as const;

  let {
    open,
    onOpenChange,
    hasFilterIndicator,
    filtersButtonLabel,
    groupedLibraryFilterOptions = EMPTY_ARRAY,
    draftLibraries = EMPTY_ARRAY,
    onDraftLibrariesChange,
    launcherFilterOptions = EMPTY_ARRAY,
    draftLaunchers = EMPTY_ARRAY,
    onDraftLaunchersChange,
    draftLauncherOrder = EMPTY_ARRAY,
    onDraftLauncherOrderChange,
    onCancel,
    onApply,
  }: Props = $props();
</script>

<Dialog {open} {onOpenChange}>
  <div class="relative inline-flex flex-none">
    <DialogTrigger
      class={buttonVariants({ variant: 'secondary', size: 'icon-sm' })}
      aria-label={filtersButtonLabel}
    >
      <FunnelIcon class="size-4.5" aria-hidden="true" />
    </DialogTrigger>

    {#if hasFilterIndicator}
      <span
        class="pointer-events-none absolute -top-0.5 -right-0.5 size-2 rounded-full bg-accent ring-2 ring-background"
        aria-hidden="true"
      ></span>
    {/if}
  </div>

  <DialogContent class="sm:max-w-lg">
    <DialogHeader>
      <DialogTitle>{DIALOG_TITLE}</DialogTitle>
    </DialogHeader>

    <LauncherFilterSection
      options={launcherFilterOptions}
      {draftLaunchers}
      {draftLauncherOrder}
      onLaunchersChange={onDraftLaunchersChange}
      onOrderChange={onDraftLauncherOrderChange}
    />

    <Separator />

    <LibraryFilterSection
      groupedOptions={groupedLibraryFilterOptions}
      {draftLibraries}
      onLibrariesChange={onDraftLibrariesChange}
    />

    <DialogFooter>
      <Button
        variant="secondary"
        size="sm"
        onclick={() => {
          onCancel?.();
        }}
      >
        Cancel
      </Button>

      <Button
        variant="default"
        size="sm"
        onclick={() => {
          onApply?.();
        }}
      >
        Apply
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
