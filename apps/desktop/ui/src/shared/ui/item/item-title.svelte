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
  data-slot="item-title"
  class={cn('flex w-fit items-center gap-2 text-sm/snug font-medium', className)}
  {...restProps}
>
  {@render children?.()}
</div>
