<script lang="ts">
  import { cn } from '@shared/utils';
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  type InfoTileElement = 'div' | 'label';
  type InfoTileTone = 'card' | 'soft';

  type Props = HTMLAttributes<HTMLElement> & {
    as?: InfoTileElement;
    label?: string;
    tone?: InfoTileTone;
    children?: Snippet;
  };

  const {
    as = 'div',
    label = '',
    tone = 'soft',
    class: className = '',
    children,
    ...rest
  }: Props = $props();

  const labelText = $derived(label.trim());
</script>

<svelte:element
  this={as}
  {...rest}
  class={cn(
    'grid min-w-0 content-start gap-2 rounded-2xl border border-border-subtle p-3',
    tone === 'card' ? 'bg-bg-card/75 shadow-sm' : 'bg-bg-soft',
    className,
  )}
>
  {#if labelText}
    <span class="text-xs font-semibold tracking-widest text-text-subtle uppercase">{labelText}</span
    >
  {/if}

  {@render children?.()}
</svelte:element>
