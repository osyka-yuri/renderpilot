<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';
  import { Surface } from '@shared/ui';

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
  class={['settings-section-shell', className]}
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

  <div class="settings-section-shell__panel">
    <Surface tone="elevated" shadow>
      {@render children?.()}
    </Surface>
  </div>
</article>

<style>
  .settings-section-shell,
  .section-header {
    display: grid;
  }

  .settings-section-shell {
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

  .settings-section-shell__panel {
    display: grid;
    gap: 0;
    overflow: hidden;
    border-radius: var(--radius-xl);
  }

  @media (max-width: 720px) {
    .section-header {
      padding-inline: 0;
    }
  }
</style>
