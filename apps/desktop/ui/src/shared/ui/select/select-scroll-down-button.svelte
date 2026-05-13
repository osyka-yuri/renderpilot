<script lang="ts">
  import { Select as SelectPrimitive } from 'bits-ui';
  import { cn } from '@shared/classnames';
  import type { WithoutChildrenOrChild } from '../types';
  import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';

  let {
    ref = $bindable(null),
    class: className,
    ...restProps
  }: WithoutChildrenOrChild<SelectPrimitive.ScrollDownButtonProps> = $props();
</script>

<SelectPrimitive.ScrollDownButton
  bind:ref
  data-slot="select-scroll-down-button"
  class={cn('flex cursor-default items-center justify-center py-1', className)}
  {...restProps}
>
  <ChevronDownIcon class="size-4" />
</SelectPrimitive.ScrollDownButton>
