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
  data-slot="empty-header"
  class={cn('flex max-w-sm flex-col items-center gap-2 text-center', className)}
  {...restProps}
>
  {@render children?.()}
</div>
