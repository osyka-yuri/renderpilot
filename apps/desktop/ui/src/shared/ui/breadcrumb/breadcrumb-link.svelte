<script lang="ts">
  import type { Snippet } from 'svelte';
  import { cn } from '@shared/classnames';
  import type { WithElementRef } from '../types';
  import type { HTMLAnchorAttributes } from 'svelte/elements';

  let {
    ref = $bindable(null),
    class: className,
    href,
    child,
    children,
    ...restProps
  }: WithElementRef<HTMLAnchorAttributes> & {
    child?: Snippet<[{ props: HTMLAnchorAttributes }]>;
  } = $props();

  const attrs = $derived({
    'data-slot': 'breadcrumb-link',
    class: cn('transition-colors hover:text-foreground', className),
    href,
    ...restProps,
  });
</script>

{#if child}
  {@render child({ props: attrs })}
{:else}
  <a bind:this={ref} {...attrs}>
    {@render children?.()}
  </a>
{/if}
