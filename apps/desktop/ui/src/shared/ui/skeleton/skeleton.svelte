<script lang="ts">
  import { cn } from '@shared/classnames';
  import type { WithElementRef, WithoutChildren } from '../types';
  import type { HTMLAttributes } from 'svelte/elements';

  let {
    ref = $bindable(null),
    class: className,
    ...restProps
  }: WithoutChildren<WithElementRef<HTMLAttributes<HTMLDivElement>>> = $props();
</script>

<div
  bind:this={ref}
  data-slot="skeleton"
  class={cn('animate-pulse rounded-md bg-accent', className)}
  {...restProps}
></div>
