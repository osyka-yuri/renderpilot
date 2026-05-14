<script lang="ts">
  import { cn } from '@shared/classnames';
  import type { WithElementRef } from '../types';
  import type { HTMLAttributes } from 'svelte/elements';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
</script>

<div
  bind:this={ref}
  data-slot="item-actions"
  class={cn('flex items-center gap-2', className)}
  {...restProps}
>
  {@render children?.()}
</div>
