<script module lang="ts">
  import type { AccordionItem } from './accordion-types';

  const DEFAULT_ID_PREFIX = 'accordion';
  const EMPTY_ITEMS: readonly AccordionItem[] = [];

  function normalizeDomIdPart(value: string): string {
    return value
      .toLowerCase()
      .trim()
      .replace(/[^a-z0-9_-]+/g, '-')
      .replace(/^-+|-+$/g, '');
  }

  function hashValue(value: string): string {
    let hash = 0;

    for (let index = 0; index < value.length; index += 1) {
      hash = (hash * 33 + value.charCodeAt(index)) >>> 0;
    }

    return hash.toString(36);
  }

  function createStableIdPart(value: string, fallbackPrefix: string): string {
    const normalizedPart = normalizeDomIdPart(value);
    const hashPart = hashValue(value);

    return normalizedPart ? `${normalizedPart}-${hashPart}` : `${fallbackPrefix}-${hashPart}`;
  }

  function createSafeIdPrefix(value: string): string {
    return normalizeDomIdPart(value) || DEFAULT_ID_PREFIX;
  }

  function createAccordionElementId(
    idPrefix: string,
    itemValue: string,
    suffix: 'trigger' | 'panel',
  ): string {
    return `${idPrefix}-${createStableIdPart(itemValue, 'item')}-${suffix}`;
  }
</script>

<script lang="ts">
  import { cn, normalizeA11yTextProps } from '@shared/utils';
  import { cubicOut } from 'svelte/easing';
  import { slide } from 'svelte/transition';
  import type { HTMLAttributes } from 'svelte/elements';
  import type { Snippet } from 'svelte';
  import Badge from './Badge.svelte';

  type NativeAccordionProps = Omit<HTMLAttributes<HTMLDivElement>, 'class' | 'aria-label'>;

  type AccordionProps = NativeAccordionProps & {
    itemContent?: Snippet<[AccordionItem]>;
    items?: readonly AccordionItem[];
    value?: string | null;
    allowCollapse?: boolean;
    idPrefix?: string;
    class?: string;
    'aria-label'?: string | null;
    onValueChange?: (value: string | null) => void;
  };

  const PANEL_TRANSITION = {
    duration: 180,
    easing: cubicOut,
  } as const;

  let {
    itemContent,
    items = EMPTY_ITEMS,
    value = $bindable(null),
    allowCollapse = true,
    idPrefix = DEFAULT_ID_PREFIX,
    class: className = '',
    'aria-label': ariaLabel = null,
    role,
    onValueChange,
    ...restProps
  }: AccordionProps = $props();

  const safeIdPrefix = $derived(createSafeIdPrefix(idPrefix));
  const accordionClass = $derived(cn('grid gap-2', className));

  const a11yText = $derived(
    normalizeA11yTextProps({
      label: ariaLabel,
    }),
  );

  const accordionRole = $derived(role ?? (a11yText.ariaLabel ? 'group' : undefined));

  function getTriggerId(itemValue: string): string {
    return createAccordionElementId(safeIdPrefix, itemValue, 'trigger');
  }

  function getPanelId(itemValue: string): string {
    return createAccordionElementId(safeIdPrefix, itemValue, 'panel');
  }

  function isExpanded(item: AccordionItem): boolean {
    return value === item.value;
  }

  function setValue(nextValue: string | null): void {
    if (nextValue === value) {
      return;
    }

    value = nextValue;
    onValueChange?.(nextValue);
  }

  function selectItem(item: AccordionItem): void {
    if (item.disabled) {
      return;
    }

    const nextValue = allowCollapse && isExpanded(item) ? null : item.value;

    setValue(nextValue);
  }

  function focusTrigger(item: AccordionItem | undefined): void {
    if (!item) {
      return;
    }

    const trigger = document.getElementById(getTriggerId(item.value));

    if (trigger instanceof HTMLButtonElement) {
      trigger.focus();
    }
  }

  function getEnabledItems(): AccordionItem[] {
    return items.filter((item) => !item.disabled);
  }

  function focusEnabledItemByOffset(currentItem: AccordionItem, offset: number): void {
    const enabledItems = getEnabledItems();

    if (enabledItems.length === 0) {
      return;
    }

    const currentIndex = enabledItems.findIndex((item) => item.value === currentItem.value);

    if (currentIndex === -1) {
      return;
    }

    const nextIndex = (currentIndex + offset + enabledItems.length) % enabledItems.length;

    focusTrigger(enabledItems[nextIndex]);
  }

  function handleTriggerKeydown(event: KeyboardEvent, item: AccordionItem): void {
    if (event.altKey || event.ctrlKey || event.metaKey) {
      return;
    }

    const enabledItems = getEnabledItems();

    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        focusEnabledItemByOffset(item, 1);
        break;

      case 'ArrowUp':
        event.preventDefault();
        focusEnabledItemByOffset(item, -1);
        break;

      case 'Home':
        event.preventDefault();
        focusTrigger(enabledItems[0]);
        break;

      case 'End':
        event.preventDefault();
        focusTrigger(enabledItems[enabledItems.length - 1]);
        break;
    }
  }
</script>

<div {...restProps} class={accordionClass} role={accordionRole} aria-label={a11yText.ariaLabel}>
  {#each items as item (item.value)}
    {@const expanded = isExpanded(item)}
    {@const triggerId = getTriggerId(item.value)}
    {@const panelId = getPanelId(item.value)}
    {@const badges = item.badges ?? []}

    <div
      class={cn('grid gap-2 rounded-2xl border border-border-subtle bg-bg-card p-2', 'shadow-sm')}
    >
      <button
        id={triggerId}
        type="button"
        class={cn(
          'group flex min-w-0 items-stretch justify-between gap-3 p-4',
          'rounded-2xl border border-border-control bg-bg-control text-left',
          'cursor-pointer transition duration-140 motion-reduce:transition-none',
          !expanded && 'hover:border-border-strong hover:bg-bg-control-hover',
          'focus-visible:ring-2 focus-visible:ring-accent',
          'focus-visible:ring-offset-2 focus-visible:ring-offset-bg-base',
          'focus-visible:outline-none',
          'disabled:cursor-not-allowed disabled:opacity-50',
          expanded && 'border-accent-outline bg-accent-soft',
        )}
        aria-expanded={expanded}
        aria-controls={panelId}
        disabled={item.disabled}
        onclick={() => {
          selectItem(item);
        }}
        onkeydown={(event) => {
          handleTriggerKeydown(event, item);
        }}
      >
        <span class="grid min-w-0 gap-1">
          <span class={cn('flex min-w-0 items-baseline justify-between gap-3', 'max-lg:flex-col')}>
            <strong class="min-w-0 text-base/tight wrap-break-word text-text-strong"
              >{item.title}</strong
            >

            {#if item.meta}
              <span class="shrink-0 text-xs/snug text-text-subtle">{item.meta}</span>
            {/if}
          </span>

          {#if item.summary}
            <span class="min-w-0 text-sm/snug wrap-break-word text-text-muted">{item.summary}</span>
          {/if}
        </span>

        <span
          class={cn(
            'flex min-w-0 shrink-0 items-center justify-end gap-2 self-center',
            'max-lg:justify-start',
          )}
        >
          {#if badges.length}
            <span class={cn('flex min-w-0 flex-wrap justify-end gap-1', 'max-lg:justify-start')}>
              {#each badges as badge, badgeIndex (`${badge.label}-${badge.tone ?? 'neutral'}-${badge.surface ?? 'outline'}-${badgeIndex}`)}
                <Badge surface={badge.surface ?? 'outline'} tone={badge.tone ?? 'neutral'}>
                  {badge.label}
                </Badge>
              {/each}
            </span>
          {/if}

          <span
            class={cn(
              'relative grid size-7 shrink-0 place-items-center',

              'rounded-2xl border border-border-subtle bg-bg-soft',
              'text-text-muted',

              'transition duration-140',
              'motion-reduce:transition-none',

              'group-hover:text-text-strong',
              'group-focus-visible:text-text-strong',

              expanded && 'text-text-strong',
              'before:block before:size-1',

              'before:border-r-2 before:border-b-2 before:border-r-current',
              'before:border-b-current',

              'before:transition-transform before:duration-140',
              'motion-reduce:before:transition-none',

              'before:-translate-y-0.5 before:rotate-45',
              expanded && 'before:translate-y-0.5 before:rotate-225',
              'max-lg:self-start',
            )}
            aria-hidden="true"
          ></span>
        </span>
      </button>

      {#if expanded}
        <div
          id={panelId}
          class="grid gap-3 p-2"
          role="region"
          aria-labelledby={triggerId}
          transition:slide={PANEL_TRANSITION}
        >
          {@render itemContent?.(item)}
        </div>
      {/if}
    </div>
  {/each}
</div>
