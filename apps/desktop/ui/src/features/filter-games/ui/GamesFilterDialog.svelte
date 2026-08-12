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
    ScrollArea,
    Separator,
    buttonVariants,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import { type GroupedLibraryFilterOptions } from '../model/library-filter-options';
  import { type LauncherFilterOption } from '../model/launcher-filter-options';
  import type { AddonCapability } from '@entities/game';
  import AddonFilterSection from './AddonFilterSection.svelte';
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
    addonOptions?: readonly AddonCapability[];
    draftAddons?: readonly string[];
    onDraftAddonsChange?: (addons: readonly string[]) => void;
    launcherFilterOptions?: readonly LauncherFilterOption[];
    draftLaunchers?: readonly string[];
    onDraftLaunchersChange?: (launchers: readonly string[]) => void;
    draftLauncherOrder?: readonly string[];
    onDraftLauncherOrderChange?: (order: readonly string[]) => void;
    onCancel?: () => void;
    onApply?: () => void;
  };

  const EMPTY_ARRAY = [] as const;

  let {
    open,
    onOpenChange,
    hasFilterIndicator,
    filtersButtonLabel,
    groupedLibraryFilterOptions = EMPTY_ARRAY,
    draftLibraries = EMPTY_ARRAY,
    onDraftLibrariesChange,
    addonOptions = EMPTY_ARRAY,
    draftAddons = EMPTY_ARRAY,
    onDraftAddonsChange,
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

  <DialogContent
    class="max-h-[calc(100dvh-2rem)] grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden"
  >
    <DialogHeader>
      <DialogTitle>{t('filters.title')}</DialogTitle>
    </DialogHeader>

    <ScrollArea type="auto" class="min-h-0">
      <div class="grid gap-4">
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

        <Separator />

        <AddonFilterSection
          options={addonOptions}
          {draftAddons}
          onAddonsChange={onDraftAddonsChange}
        />
      </div>
    </ScrollArea>

    <DialogFooter>
      <Button
        variant="secondary"
        size="sm"
        onclick={() => {
          onCancel?.();
        }}
      >
        {t('common.cancel')}
      </Button>

      <Button
        variant="default"
        size="sm"
        onclick={() => {
          onApply?.();
        }}
      >
        {t('common.apply')}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
