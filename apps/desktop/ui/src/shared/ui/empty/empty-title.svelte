<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements';
  import { cn } from '@shared/classnames';
  import type { WithElementRef } from '../types';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
</script>

<div
  bind:this={ref}
  data-slot="empty-title"
  class={cn('text-lg font-medium tracking-tight', className)}
  {...restProps}
>
  {@render children?.()}
</div>
