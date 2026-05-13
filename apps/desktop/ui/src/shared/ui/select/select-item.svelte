<script lang="ts">
  import { Select as SelectPrimitive } from 'bits-ui';
  import { cn } from '@shared/classnames';
  import type { WithoutChild } from '../types';
  import CheckIcon from '@lucide/svelte/icons/check';

  type ItemSnippetProps = {
    selected: boolean;
    highlighted: boolean;
  };

  let {
    ref = $bindable(null),
    class: className,
    value,
    label,
    children: childrenProp,
    ...restProps
  }: WithoutChild<SelectPrimitive.ItemProps> = $props();
</script>

<SelectPrimitive.Item
  bind:ref
  {value}
  data-slot="select-item"
  class={cn(
    "relative flex w-full cursor-default items-center gap-2 rounded-sm py-1.5 ps-2 pe-8 text-sm outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-50 data-highlighted:bg-accent data-highlighted:text-accent-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4 [&_svg:not([class*='text-'])]:text-muted-foreground *:[span]:last:flex *:[span]:last:items-center *:[span]:last:gap-2",
    className,
  )}
  {...restProps}
>
  {#snippet children(snippetProps: ItemSnippetProps)}
    <span class="absolute inset-e-2 flex size-3.5 items-center justify-center">
      {#if snippetProps.selected}
        <CheckIcon class="size-4" />
      {/if}
    </span>
    {#if childrenProp}
      {@render childrenProp(snippetProps)}
    {:else}
      {label ?? value}
    {/if}
  {/snippet}
</SelectPrimitive.Item>
