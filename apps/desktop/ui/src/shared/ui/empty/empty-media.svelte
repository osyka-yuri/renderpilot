<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements';
  import { cn } from '@shared/classnames';
  import type { WithElementRef } from '../types';
  import { emptyMediaVariants, type EmptyMediaVariant } from './empty-media-variants';

  let {
    ref = $bindable(null),
    class: className,
    children,
    variant = 'default',
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLDivElement>> & { variant?: EmptyMediaVariant } = $props();
</script>

<div
  bind:this={ref}
  data-slot="empty-icon"
  data-variant={variant}
  class={cn(emptyMediaVariants({ variant }), className)}
  {...restProps}
>
  {@render children?.()}
</div>
