<script context="module" lang="ts">
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
</script>

<script lang="ts">
  import { cubicOut } from 'svelte/easing';
  import { slide } from 'svelte/transition';
  import { cx } from '@shared/utils/cx';
  import Badge from './Badge.svelte';

  export let items: AccordionItem[] = [];
  export let value: string | null = null;
  export let ariaLabel = 'Accordion';
  export let allowCollapse = true;
  export let idPrefix = 'accordion';
  export let onValueChange: ((value: string | null) => void) | undefined = undefined;

  let className = '';
  export { className as class };

  function isExpanded(item: AccordionItem): boolean {
    return value === item.value;
  }

  function getSafeIdPart(value: string): string {
    return value
      .toLowerCase()
      .trim()
      .replace(/[^a-z0-9_-]+/g, '-')
      .replace(/^-+|-+$/g, '');
  }

  function getItemId(idPart: string, suffix: 'trigger' | 'panel'): string {
    return `${idPrefix}-${idPart}-${suffix}`;
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

  function handleTriggerClick(event: MouseEvent): void {
    const trigger = event.currentTarget as HTMLButtonElement;
    const itemValue = trigger.dataset.value;

    if (!itemValue) {
      return;
    }

    const item = items.find((candidate) => candidate.value === itemValue);

    if (!item) {
      return;
    }

    selectItem(item);
  }
</script>

<div class={cx('accordion', className)} aria-label={ariaLabel}>
  {#each items as item, itemIndex (item.value)}
    {@const expanded = isExpanded(item)}
    {@const idPart = getSafeIdPart(item.value) || `item-${itemIndex}`}
    {@const triggerId = getItemId(idPart, 'trigger')}
    {@const panelId = getItemId(idPart, 'panel')}

    <section class="accordion-item" data-expanded={expanded ? 'true' : undefined}>
      <button
        id={triggerId}
        type="button"
        class="accordion-trigger"
        data-value={item.value}
        aria-expanded={expanded}
        aria-controls={panelId}
        disabled={item.disabled}
        onclick={handleTriggerClick}
      >
        <span class="accordion-copy">
          <span class="accordion-title-row">
            <strong>{item.title}</strong>

            {#if item.meta}
              <span class="accordion-meta">{item.meta}</span>
            {/if}
          </span>

          {#if item.summary}
            <span class="accordion-summary">{item.summary}</span>
          {/if}
        </span>

        <span class="accordion-side">
          {#if item.badges?.length}
            <span class="accordion-badges" aria-label="Badges">
              {#each item.badges as badge, badgeIndex (`${badge.label}-${badgeIndex}`)}
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
          transition:slide={{ duration: 180, easing: cubicOut }}
        >
          <slot {item} />
        </div>
      {/if}
    </section>
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
    background: var(--bg-control);
    color: inherit;
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

  .accordion-title-row strong {
    min-width: 0;
    color: var(--text-strong);
    font-size: 1.05rem;
    line-height: 1.2;
  }

  .accordion-meta {
    flex-shrink: 0;
    color: var(--text-subtle);
    font-size: 0.8125rem;
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
  }

  .accordion-chevron::before {
    content: '';
    width: 0.48rem;
    height: 0.48rem;
    border-right: 1.75px solid currentColor;
    border-bottom: 1.75px solid currentColor;
    transform: translateY(-0.12rem) rotate(45deg);
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
</style>
