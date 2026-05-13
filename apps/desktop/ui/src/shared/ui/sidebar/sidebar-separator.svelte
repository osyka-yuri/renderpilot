<script lang="ts">
  import { Separator } from '../separator/index.js';
  import { cn } from '@shared/classnames';

  let {
    ref = $bindable(null),
    class: className,
    ...restProps
  }: {
    ref?: HTMLElement | null;
    class?: string;
    [key: string]: unknown;
  } = $props();
</script>

<Separator
  bind:ref
  data-slot="sidebar-separator"
  data-sidebar="separator"
  class={cn('bg-sidebar-border', className)}
  {...restProps as Record<string, unknown>}
/>
