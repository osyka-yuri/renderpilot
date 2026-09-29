<script lang="ts">
  import { RadioGroup as RadioGroupPrimitive } from 'bits-ui';
  import { cn } from '@shared/classnames';
  import type { Snippet } from 'svelte';
  import type { WithoutChild } from '../types';

  import type { SegmentedControlSize } from './segmented-control.context';
  import { getSegmentedControlCtx } from './segmented-control.context';

  type SegmentedControlItemProps = Omit<WithoutChild<RadioGroupPrimitive.ItemProps>, 'children'> & {
    children?: Snippet<[{ checked: boolean }]>;
  };

  const SIZE_CLASSES: Record<SegmentedControlSize, string> = {
    default: 'h-7 px-3 text-sm',
    sm: 'h-7 px-2 text-xs',
  };

  const ITEM_CLASSES =
    'inline-flex min-w-0 shrink-0 items-center justify-center gap-2 rounded-md border border-transparent bg-transparent font-medium whitespace-nowrap text-muted-foreground transition-[color,background-color,box-shadow] outline-none hover:bg-background/70 hover:text-foreground focus-visible:z-10 focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:ring-offset-2 focus-visible:ring-offset-muted disabled:pointer-events-none disabled:opacity-50 aria-invalid:ring-destructive/20 data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground data-[state=checked]:shadow-sm data-[state=checked]:hover:bg-primary/90 data-[state=checked]:hover:text-primary-foreground dark:aria-invalid:ring-destructive/40';

  const ICON_CLASSES =
    "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4";

  let {
    ref = $bindable(null),
    value,
    class: className,
    children: childrenProp,
    ...itemProps
  }: SegmentedControlItemProps = $props();

  const ctx = getSegmentedControlCtx();
  const itemClassName = $derived(cn(ITEM_CLASSES, ICON_CLASSES, SIZE_CLASSES[ctx.size], className));
</script>

<RadioGroupPrimitive.Item
  bind:ref
  {value}
  data-slot="segmented-control-item"
  data-size={ctx.size}
  class={itemClassName}
  {...itemProps}
>
  {#snippet children({ checked }: { checked: boolean })}
    {@render childrenProp?.({ checked })}
  {/snippet}
</RadioGroupPrimitive.Item>
