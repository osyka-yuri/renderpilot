<script lang="ts">
  import { Input } from '../input/index.js';
  import { cn } from '@shared/classnames';

  let {
    ref = $bindable(null),
    value = $bindable(''),
    class: className,
    ...restProps
  }: {
    ref?: HTMLElement | null;
    value?: string;
    class?: string;
    restProps?: Record<string, unknown>;
  } = $props();
</script>

<Input
  bind:ref
  bind:value
  data-slot="sidebar-input"
  data-sidebar="input"
  class={cn('h-8 w-full bg-background shadow-none', className)}
  {...restProps}
/>
