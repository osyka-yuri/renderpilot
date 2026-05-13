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
  data-slot="card-footer"
  class={cn('flex items-center px-6', className)}
  {...restProps}
>
  {@render children?.()}
</div>
