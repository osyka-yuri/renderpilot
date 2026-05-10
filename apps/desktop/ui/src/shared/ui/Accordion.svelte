<script module lang="ts">
  import type { BadgeSurface, BadgeTone } from './Badge.svelte';

  export type AccordionBadge = {
    label: string;
    tone?: BadgeTone;
    surface?: BadgeSurface;
  };

  export type AccordionItem = {
    value: string;
    title: string;
    summary?: string;
    meta?: string;
    badges?: AccordionBadge[];
    disabled?: boolean;
  };

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
  import { cubicOut } from 'svelte/easing';
  import { slide } from 'svelte/transition';
  import { normalizeA11yTextProps } from '@shared/utils/a11y';
  import { cx } from '@shared/utils/cx';
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
  const accordionClass = $derived(cx('accordion', className));

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

    <div class="accordion-item" data-expanded={expanded ? 'true' : undefined}>
      <button
        id={triggerId}
        type="button"
        class="accordion-trigger"
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
        <span class="accordion-copy">
          <span class="accordion-title-row">
            <strong class="accordion-title">{item.title}</strong>

            {#if item.meta}
              <span class="accordion-meta">{item.meta}</span>
            {/if}
          </span>

          {#if item.summary}
            <span class="accordion-summary">{item.summary}</span>
          {/if}
        </span>

        <span class="accordion-side">
          {#if badges.length}
            <span class="accordion-badges">
              {#each badges as badge, badgeIndex (`${badge.label}-${badge.tone ?? 'neutral'}-${badge.surface ?? 'outline'}-${badgeIndex}`)}
                <Badge surface={badge.surface ?? 'outline'} tone={badge.tone ?? 'neutral'}>
                  {badge.label}
                </Badge>
              {/each}
            </span>
          {/if}

          <span class="accordion-chevron" aria-hidden="true"></span>
        </span>
      </button>

      {#if expanded}
        <div
          id={panelId}
          class="accordion-panel"
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

<style>
  .accordion {
    display: grid;
    gap: var(--space-2);
  }

  .accordion-item {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-xl);
    background: var(--bg-card);
    box-shadow: var(--shadow-card);
  }

  .accordion-trigger {
    width: 100%;
    min-width: 0;
    display: flex;
    align-items: stretch;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-4);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-lg);
    appearance: none;
    background: var(--bg-control);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      background 140ms ease,
      border-color 140ms ease,
      box-shadow 140ms ease,
      opacity 140ms ease;
  }

  .accordion-trigger:hover:not(:disabled) {
    background: var(--bg-control-hover);
    border-color: var(--border-strong);
  }

  .accordion-trigger:focus-visible {
    outline: none;
    box-shadow: var(--shadow-focus);
  }

  .accordion-trigger:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .accordion-item[data-expanded='true'] .accordion-trigger {
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--accent-soft) 72%, var(--bg-control)),
      var(--bg-control)
    );
    border-color: var(--accent-outline);
  }

  .accordion-copy {
    min-width: 0;
    display: grid;
    gap: var(--space-1);
  }

  .accordion-title-row {
    min-width: 0;
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .accordion-title {
    min-width: 0;
    color: var(--text-strong);
    font-size: 1.05rem;
    line-height: 1.2;
    overflow-wrap: anywhere;
  }

  .accordion-meta {
    flex-shrink: 0;
    color: var(--text-subtle);
    font-size: 0.8125rem;
    line-height: 1.3;
  }

  .accordion-summary {
    min-width: 0;
    color: var(--text-muted);
    font-size: 0.84rem;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }

  .accordion-side {
    min-width: 0;
    flex-shrink: 0;
    align-self: center;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  .accordion-badges {
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-1);
  }

  .accordion-chevron {
    width: 1.75rem;
    height: 1.75rem;
    flex: 0 0 auto;
    display: grid;
    place-items: center;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-soft);
    color: var(--text-muted);
    transition:
      color 140ms ease,
      border-color 140ms ease,
      background 140ms ease;
  }

  .accordion-chevron::before {
    content: '';
    width: 0.48rem;
    height: 0.48rem;
    border-right: 1.75px solid currentColor;
    border-bottom: 1.75px solid currentColor;
    transform: translateY(-0.12rem) rotate(45deg);
    transition: transform 140ms ease;
  }

  .accordion-trigger:hover:not(:disabled) .accordion-chevron,
  .accordion-trigger:focus-visible .accordion-chevron,
  .accordion-item[data-expanded='true'] .accordion-chevron {
    color: var(--text-strong);
  }

  .accordion-item[data-expanded='true'] .accordion-chevron::before {
    transform: translateY(0.12rem) rotate(225deg);
  }

  .accordion-panel {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-2);
  }

  @media (max-width: 820px) {
    .accordion-trigger,
    .accordion-title-row {
      flex-direction: column;
    }

    .accordion-side,
    .accordion-badges {
      justify-content: flex-start;
    }

    .accordion-chevron {
      align-self: flex-start;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .accordion-trigger,
    .accordion-chevron,
    .accordion-chevron::before {
      transition: none;
    }
  }
</style>
