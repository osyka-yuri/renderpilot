<script lang="ts">
  import { cn } from '@shared/classnames';
  import type { WithElementRef } from '../types';
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import { useSidebar } from './context.svelte.js';

  type Props = WithElementRef<
    Omit<HTMLButtonAttributes, 'aria-label' | 'children' | 'onclick' | 'tabindex' | 'type'>,
    HTMLButtonElement
  > & {
    label: string;
  };

  let { ref = $bindable(null), class: className, label, ...restProps }: Props = $props();

  const sidebar = useSidebar();
</script>

<button
  bind:this={ref}
  type="button"
  data-sidebar="rail"
  data-slot="sidebar-rail"
  tabindex={-1}
  onclick={sidebar.toggle}
  class={cn(
    'absolute inset-y-0 z-20 hidden w-4 transition-colors ease-linear group-data-[side=left]:-inset-e-2 group-data-[side=right]:-inset-s-2 after:absolute after:inset-y-0 after:inset-s-[calc(1/2*100%-1px)] after:w-[2px] hover:after:bg-sidebar-border sm:flex',
    'in-data-[side=left]:cursor-w-resize in-data-[side=right]:cursor-e-resize rtl:in-data-[side=left]:cursor-e-resize rtl:in-data-[side=right]:cursor-w-resize',
    '[[data-side=left][data-state=collapsed]_&]:cursor-e-resize rtl:[[data-side=left][data-state=collapsed]_&]:cursor-w-resize [[data-side=right][data-state=collapsed]_&]:cursor-w-resize rtl:[[data-side=right][data-state=collapsed]_&]:cursor-e-resize',
    'group-data-[collapsible=offcanvas]:after:inset-s-full hover:group-data-[collapsible=offcanvas]:bg-sidebar',
    className,
  )}
  {...restProps}
>
  <span class="sr-only">{label}</span>
</button>
