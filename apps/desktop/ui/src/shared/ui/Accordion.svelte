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
  import Badge from './Badge.svelte';

  export let items: AccordionItem[] = [];
  export let value: string | null = null;
  export let ariaLabel = 'Accordion';
  export let allowCollapse = true;
  export let onValueChange: ((value: string | null) => void) | undefined = undefined;

  let className = '';
  export { className as class };

  function isExpanded(item: AccordionItem): boolean {
    return value === item.value;
  }

  function selectItem(item: AccordionItem): void {
    if (item.disabled) {
      return;
    }

    const nextValue = allowCollapse && isExpanded(item) ? null : item.value;

    onValueChange?.(nextValue);
  }
</script>

<div class={['accordion', className].filter(Boolean).join(' ')} aria-label={ariaLabel}>
  {#each items as item (item.value)}
    <section class="accordion-item" data-expanded={isExpanded(item) ? 'true' : undefined}>
      <button
        type="button"
        class="accordion-trigger"
        aria-expanded={isExpanded(item)}
        disabled={item.disabled}
        onclick={() => selectItem(item)}
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
            <span class="accordion-badges">
              {#each item.badges as badge}
                <Badge surface={badge.surface ?? 'outline'} tone={badge.tone ?? 'neutral'}>
                  {badge.label}
                </Badge>
              {/each}
            </span>
          {/if}

          <span class="accordion-chevron" aria-hidden="true"></span>
        </span>
      </button>

      {#if isExpanded(item)}
        <div class="accordion-panel" transition:slide={{ duration: 180, easing: cubicOut }}>
          <slot item={item} />
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
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    align-items: stretch;
    min-width: 0;
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

  .accordion-item[data-expanded] .accordion-trigger {
    background:
      linear-gradient(180deg, color-mix(in srgb, var(--accent-soft) 72%, var(--bg-control)), var(--bg-control));
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
    gap: var(--space-3);
    align-items: baseline;
    justify-content: space-between;
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
    display: flex;
    align-items: center;
    align-self: center;
    justify-content: flex-end;
    gap: var(--space-2);
    min-width: 0;
    flex-shrink: 0;
  }

  .accordion-badges {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-1);
    min-width: 0;
  }

  .accordion-chevron {
    width: 1.75rem;
    height: 1.75rem;
    display: grid;
    place-items: center;
    flex: 0 0 auto;
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
  .accordion-item[data-expanded] .accordion-chevron {
    color: var(--text-strong);
  }

  .accordion-item[data-expanded] .accordion-chevron::before {
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
