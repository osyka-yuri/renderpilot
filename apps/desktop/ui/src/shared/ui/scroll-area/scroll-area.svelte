<script lang="ts">
  import { ScrollArea as ScrollAreaPrimitive } from 'bits-ui';
  import Scrollbar from './scroll-area-scrollbar.svelte';
  import { cn } from '@shared/classnames';
  import type { WithoutChild } from '../types';

  type ViewportRegion = { label: string } | { labelledBy: string };

  let {
    ref = $bindable(null),
    viewportRef = $bindable(null),
    class: className,
    orientation = 'vertical',
    scrollbarXClasses = '',
    scrollbarYClasses = '',
    viewportRegion,
    viewportFocusable = false,
    children,
    ...restProps
  }: WithoutChild<ScrollAreaPrimitive.RootProps> & {
    orientation?: 'vertical' | 'horizontal' | 'both' | undefined;
    scrollbarXClasses?: string | undefined;
    scrollbarYClasses?: string | undefined;
    viewportRef?: HTMLElement | null;
    viewportRegion?: ViewportRegion;
    viewportFocusable?: boolean;
  } = $props();

  const viewportAriaProps = $derived(
    viewportRegion === undefined
      ? {}
      : 'label' in viewportRegion
        ? { role: 'region' as const, 'aria-label': viewportRegion.label }
        : { role: 'region' as const, 'aria-labelledby': viewportRegion.labelledBy },
  );
</script>

<ScrollAreaPrimitive.Root
  bind:ref
  data-slot="scroll-area"
  class={cn('relative', className)}
  {...restProps}
>
  <ScrollAreaPrimitive.Viewport
    {...viewportAriaProps}
    tabindex={viewportFocusable ? 0 : undefined}
    bind:ref={viewportRef}
    data-slot="scroll-area-viewport"
    class={cn(
      'size-full rounded-[inherit] ring-ring/10 transition-[color,box-shadow] focus-visible:ring-4 focus-visible:outline-2 dark:ring-ring/20',
      (orientation === 'vertical' || orientation === 'both') && 'pe-3',
      (orientation === 'horizontal' || orientation === 'both') && 'pb-3',
    )}
  >
    {@render children?.()}
  </ScrollAreaPrimitive.Viewport>
  {#if orientation === 'vertical' || orientation === 'both'}
    <Scrollbar orientation="vertical" class={scrollbarYClasses} />
  {/if}
  {#if orientation === 'horizontal' || orientation === 'both'}
    <Scrollbar orientation="horizontal" class={scrollbarXClasses} />
  {/if}
  <ScrollAreaPrimitive.Corner />
</ScrollAreaPrimitive.Root>
