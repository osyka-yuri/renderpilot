<script lang="ts">
  import { Tabs as TabsPrimitive } from 'bits-ui';
  import { cn } from '@shared/classnames';
  import type { WithoutChild } from '../types';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithoutChild<TabsPrimitive.ContentProps> = $props();
</script>

<TabsPrimitive.Content
  bind:ref
  data-slot="tabs-content"
  class={cn('flex-1 outline-none', className)}
  {...restProps}
>
  {@render children?.()}
</TabsPrimitive.Content>
