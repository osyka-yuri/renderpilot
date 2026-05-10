<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';
  import Surface from '@shared/ui/Surface.svelte';

  type Props = HTMLAttributes<HTMLElement> & {
    titleId?: string;
    eyebrow?: string;
    title?: string;
    description?: string;
    children?: Snippet;
  };

  let {
    titleId = '',
    eyebrow = '',
    title = '',
    description = '',
    children,
    class: className = '',
    ...rest
  }: Props = $props();

  const toOptionalText = (value: string): string | undefined => {
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : undefined;
  };

  const eyebrowText = $derived(toOptionalText(eyebrow));
  const titleText = $derived(toOptionalText(title));
  const descriptionText = $derived(toOptionalText(description));
  const normalizedTitleId = $derived(toOptionalText(titleId));

  const headingId = $derived(titleText ? normalizedTitleId : undefined);
  const articleLabel = $derived(titleText && !headingId ? titleText : undefined);
  const hasHeader = $derived(Boolean(eyebrowText ?? titleText ?? descriptionText));
</script>

<article
  {...rest}
  class={['settings-section settings-section-shell', className]}
  aria-labelledby={headingId}
  aria-label={articleLabel}
>
  {#if hasHeader}
    <header class="section-header">
      {#if eyebrowText}
        <p class="eyebrow">{eyebrowText}</p>
      {/if}

      {#if titleText}
        <h3 id={headingId}>{titleText}</h3>
      {/if}

      {#if descriptionText}
        <p class="section-copy">{descriptionText}</p>
      {/if}
    </header>
  {/if}

  <Surface class="settings-panel" tone="elevated" shadow>
    {@render children?.()}
  </Surface>
</article>

<style>
  .settings-section,
  .section-header {
    display: grid;
  }

  .settings-section {
    gap: var(--space-3);
  }

  .section-header {
    gap: var(--space-1);
    padding-inline: var(--space-1);
  }

  .eyebrow {
    margin: 0;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  h3 {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 600;
  }

  .section-copy {
    max-width: 56rem;
    margin: 0;
    font-size: 0.875rem;
    line-height: 1.45;
  }

  .settings-section-shell :global(.settings-panel) {
    display: grid;
    gap: 0;
    overflow: hidden;
    border-radius: var(--radius-xl);
  }

  .settings-section-shell :global(.settings-panel > :last-child) {
    border-bottom: 0;
  }

  .settings-section-shell :global(.setting-row) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4);
    border-bottom: 1px solid var(--border-subtle);
  }

  .settings-section-shell :global(.setting-copy) {
    display: grid;
    flex: 1;
    min-width: 0;
    gap: var(--space-1);
  }

  .settings-section-shell :global(.setting-label) {
    margin: 0;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .settings-section-shell :global(.setting-copy h4),
  .settings-section-shell :global(.row-title) {
    margin: 0;
    color: var(--text-strong);
    font-size: 0.95rem;
    font-weight: 600;
  }

  .settings-section-shell :global(.setting-copy p:not(.setting-label)),
  .settings-section-shell :global(.row-copy) {
    margin: 0;
    font-size: 0.84rem;
    line-height: 1.45;
  }

  @media (max-width: 720px) {
    .section-header {
      padding-inline: 0;
    }

    .settings-section-shell :global(.setting-row) {
      flex-direction: column;
      align-items: stretch;
      gap: 0.75rem;
    }
  }
</style>
