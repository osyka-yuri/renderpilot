<script lang="ts">
  import GripVerticalIcon from '@lucide/svelte/icons/grip-vertical';
  import { dragHandleZone, dragHandle } from 'svelte-dnd-action';
  import { Label, Switch } from '@shared/ui';
  import { type LauncherFilterOption } from '../model/launcher-filter-options';

  type DndLauncherItem = {
    id: string;
    value: string;
    label: string;
  };

  type DndItemsEvent = CustomEvent<{
    items: DndLauncherItem[];
  }>;

  type Props = {
    options?: readonly LauncherFilterOption[];
    draftLaunchers?: readonly string[];
    draftLauncherOrder?: readonly string[];
    onLaunchersChange?: (launchers: readonly string[]) => void;
    onOrderChange?: (order: readonly string[]) => void;
  };

  const LAUNCHERS_LABEL = 'Launchers';
  const LAUNCHERS_HEADING_ID = 'launcher-filters-heading';
  const EMPTY_LAUNCHERS_LABEL = 'No launchers detected';

  const DND_FLIP_DURATION_MS = 150;
  const EMPTY_ARRAY = [] as const;

  let {
    options = EMPTY_ARRAY,
    draftLaunchers = EMPTY_ARRAY,
    draftLauncherOrder = EMPTY_ARRAY,
    onLaunchersChange,
    onOrderChange,
  }: Props = $props();

  function isSelected(values: readonly string[], value: string): boolean {
    return values.includes(value);
  }

  function findOptionByValue(
    opts: readonly LauncherFilterOption[],
    value: string,
  ): LauncherFilterOption | undefined {
    return opts.find((option) => option.value === value);
  }

  function hasDndItemValue(items: readonly DndLauncherItem[], value: string): boolean {
    return items.some((item) => item.value === value);
  }

  function toDndItem(option: LauncherFilterOption): DndLauncherItem {
    return {
      id: option.value,
      value: option.value,
      label: option.label,
    };
  }

  function appendKnownDndItem(
    items: DndLauncherItem[],
    opts: readonly LauncherFilterOption[],
    value: string,
  ): void {
    if (hasDndItemValue(items, value)) {
      return;
    }

    const option = findOptionByValue(opts, value);

    if (!option) {
      return;
    }

    items.push(toDndItem(option));
  }

  function buildDndItems(
    preferredOrder: readonly string[],
    opts: readonly LauncherFilterOption[],
  ): DndLauncherItem[] {
    const items: DndLauncherItem[] = [];

    for (const value of preferredOrder) {
      appendKnownDndItem(items, opts, value);
    }

    for (const option of opts) {
      appendKnownDndItem(items, opts, option.value);
    }

    return items;
  }

  function getDndOrder(items: readonly DndLauncherItem[]): string[] {
    return items.map((item) => item.value);
  }

  function normalizeDndItems(
    items: readonly DndLauncherItem[],
    opts: readonly LauncherFilterOption[],
  ): DndLauncherItem[] {
    return buildDndItems(getDndOrder(items), opts);
  }

  function getNextSelectedValues(
    values: readonly string[],
    value: string,
    checked: boolean,
  ): readonly string[] | undefined {
    const selected = isSelected(values, value);

    if (selected === checked) {
      return undefined;
    }

    if (checked) {
      return [...values, value];
    }

    return values.filter((item) => item !== value);
  }

  function handleLauncherToggle(value: string, checked: boolean): void {
    const nextLaunchers = getNextSelectedValues(draftLaunchers, value, checked);

    if (!nextLaunchers) {
      return;
    }

    onLaunchersChange?.(nextLaunchers);
  }

  let dndItems: DndLauncherItem[] = $state([]);
  let prevDraftLauncherOrder: readonly string[] | undefined;
  let prevOptions: readonly LauncherFilterOption[] | undefined;

  $effect(() => {
    if (draftLauncherOrder !== prevDraftLauncherOrder || options !== prevOptions) {
      dndItems = buildDndItems(draftLauncherOrder, options);
      prevDraftLauncherOrder = draftLauncherOrder;
      prevOptions = options;
    }
  });

  function handleDndConsider(event: DndItemsEvent): void {
    dndItems = event.detail.items;
  }

  function handleDndFinalize(event: DndItemsEvent): void {
    const finalizedItems = normalizeDndItems(event.detail.items, options);
    const finalizedOrder = getDndOrder(finalizedItems);

    dndItems = finalizedItems;

    onOrderChange?.(finalizedOrder);
  }
</script>

<section class="grid gap-3" aria-labelledby={LAUNCHERS_HEADING_ID}>
  <h3 id={LAUNCHERS_HEADING_ID} class="text-sm font-medium">
    {LAUNCHERS_LABEL}
  </h3>

  {#if options.length > 0}
    <div
      class="grid gap-3"
      use:dragHandleZone={{
        items: dndItems,
        flipDurationMs: DND_FLIP_DURATION_MS,
        dropTargetStyle: {},
      }}
      onconsider={handleDndConsider}
      onfinalize={handleDndFinalize}
    >
      {#each dndItems as item (item.id)}
        {@const switchId = `launcher-switch-${item.value}`}

        <div class="flex items-center justify-between gap-3">
          <div class="flex min-w-0 items-center gap-2">
            <button
              type="button"
              use:dragHandle
              class="shrink-0 cursor-grab rounded-sm text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring active:cursor-grabbing"
              aria-label={`Reorder ${item.label}`}
            >
              <GripVerticalIcon class="size-4" aria-hidden="true" />
            </button>

            <Label for={switchId} class="truncate">
              {item.label}
            </Label>
          </div>

          <Switch
            id={switchId}
            checked={isSelected(draftLaunchers, item.value)}
            onCheckedChange={(checked: boolean) => {
              handleLauncherToggle(item.value, checked);
            }}
          />
        </div>
      {/each}
    </div>
  {:else}
    <p class="text-sm text-muted-foreground">
      {EMPTY_LAUNCHERS_LABEL}
    </p>
  {/if}
</section>
