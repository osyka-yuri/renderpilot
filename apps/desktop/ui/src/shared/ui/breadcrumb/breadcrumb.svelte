<script lang="ts">
  import type { WithElementRef } from '../types';
  import type { HTMLAttributes } from 'svelte/elements';

  let {
    ref = $bindable(null),
    class: className,
    label,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLElement>> & { label: string } = $props();
</script>

<nav bind:this={ref} data-slot="breadcrumb" class={className} aria-label={label} {...restProps}>
  {@render children?.()}
</nav>
