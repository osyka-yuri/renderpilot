<script lang="ts">
  import { cn } from '@shared/utils';
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  type Props = HTMLAttributes<HTMLElement> & {
    eyebrow?: string;
    title?: string;
    titleId?: string;
    titleTag?: 'h1' | 'h2' | 'h3' | 'h4' | 'h5' | 'h6';
    description?: string;
    children?: Snippet;
  };

  const {
    eyebrow = '',
    title = '',
    titleId,
    titleTag = 'h3',
    description = '',
    class: className = '',
    children,
    ...rest
  }: Props = $props();

  const eyebrowText = $derived(eyebrow.trim());
  const titleText = $derived(title.trim());
  const descriptionText = $derived(description.trim());
  const hasAside = $derived(Boolean(children));
</script>

<header
  {...rest}
  class={cn(
    'flex items-end justify-between gap-4 px-1',
    'max-lg:flex-col max-lg:items-start',
    className,
  )}
>
  <div class="min-w-0">
    {#if eyebrowText}
      <p class="mb-1 text-xs tracking-widest text-text-subtle uppercase">{eyebrowText}</p>
    {/if}

    {#if titleText}
      <svelte:element this={titleTag} id={titleId} class="text-base/tight font-semibold">
        {titleText}
      </svelte:element>
    {/if}

    {#if descriptionText}
      <p class="mt-1 text-sm/snug text-text-muted">{descriptionText}</p>
    {/if}
  </div>

  {#if hasAside}
    <div class="shrink-0 max-lg:shrink">{@render children?.()}</div>
  {/if}
</header>
