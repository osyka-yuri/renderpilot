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
    Label,
    Separator,
    Switch,
    ToggleGroup,
    ToggleGroupItem,
    buttonVariants,
  } from '@shared/ui';
  import {
    mergeVendorDraftLibraries,
    selectedLibrariesForVendor,
    type GroupedLibraryFilterOptions,
  } from '../model/library-filter-options';
  import { type LauncherFilterOption } from '../model/launcher-filter-options';

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
    onCancel?: () => void;
    onApply?: () => void;
  };

  const DIALOG_TITLE = 'Filters';
  const LAUNCHERS_LABEL = 'Launchers';
  const EMPTY_LAUNCHERS_LABEL = 'No launchers detected';
  const LIBRARIES_LABEL = 'Libraries';
  const EMPTY_LIBRARIES_LABEL = 'No libraries detected';

  const {
    open,
    onOpenChange,
    hasFilterIndicator,
    filtersButtonLabel,
    groupedLibraryFilterOptions = [],
    draftLibraries = [],
    onDraftLibrariesChange,
    launcherFilterOptions = [],
    draftLaunchers = [],
    onDraftLaunchersChange,
    onCancel,
    onApply,
  }: Props = $props();

  function handleGroupValueChange(vendorOptions: { value: string }[], nextValue: string[]): void {
    onDraftLibrariesChange?.(mergeVendorDraftLibraries(draftLibraries, vendorOptions, nextValue));
  }

  function groupValue(vendorOptions: { value: string }[]): string[] {
    return selectedLibrariesForVendor(draftLibraries, vendorOptions);
  }

  function isLauncherSelected(value: string): boolean {
    return draftLaunchers.includes(value);
  }

  function handleLauncherToggle(value: string, checked: boolean): void {
    const hasValue = draftLaunchers.includes(value);

    if (checked && !hasValue) {
      onDraftLaunchersChange?.([...draftLaunchers, value]);
    } else if (!checked && hasValue) {
      onDraftLaunchersChange?.(draftLaunchers.filter((launcher) => launcher !== value));
    }
  }
</script>

<Dialog {open} onOpenChange={onOpenChange}>
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

    <h3 class="text-sm font-medium">{LAUNCHERS_LABEL}</h3>

    {#if launcherFilterOptions.length > 0}
      <div class="grid gap-3">
        {#each launcherFilterOptions as option (option.value)}
          {@const switchId = `launcher-switch-${option.value}`}
          <div class="flex items-center justify-between gap-3">
            <Label for={switchId}>{option.label}</Label>
            <Switch
              id={switchId}
              checked={isLauncherSelected(option.value)}
              onCheckedChange={(checked: boolean) => {
                handleLauncherToggle(option.value, checked);
              }}
            />
          </div>
        {/each}
      </div>
    {:else}
      <span class="text-sm text-muted-foreground">{EMPTY_LAUNCHERS_LABEL}</span>
    {/if}

    <Separator />

    <h3 class="text-sm font-medium">{LIBRARIES_LABEL}</h3>

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
                <ToggleGroupItem value={option.value} class="flex-1" size="sm">
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

    <DialogFooter>
      <Button variant="secondary" size="sm" onclick={onCancel}>Cancel</Button>
      <Button variant="default" size="sm" onclick={onApply}>Apply</Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
