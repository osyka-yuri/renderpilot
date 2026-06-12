<script lang="ts">
  import { Progress as ProgressPrimitive } from 'bits-ui';
  import { cn } from '@shared/classnames';
  import type { WithoutChildrenOrChild } from '../types';

  let {
    ref = $bindable(null),
    class: className,
    max = 100,
    value,
    ...restProps
  }: WithoutChildrenOrChild<ProgressPrimitive.RootProps> = $props();

  const translateX = $derived.by(() => {
    if (max <= 0) return -100;
    const clamped = Math.min(Math.max(value ?? 0, 0), max);
    return (100 * clamped) / max - 100;
  });
</script>

<ProgressPrimitive.Root
  bind:ref
  data-slot="progress"
  class={cn('relative h-2 w-full overflow-hidden rounded-full bg-primary/20', className)}
  {value}
  {max}
  {...restProps}
>
  <div
    data-slot="progress-indicator"
    class="size-full flex-1 bg-primary transition-all"
    style="transform: translateX({translateX}%)"
  ></div>
</ProgressPrimitive.Root>
