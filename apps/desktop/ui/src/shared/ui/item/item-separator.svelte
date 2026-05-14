<script lang="ts">
  import { Separator } from '../separator';
  import { cn } from '@shared/classnames';
  import type { ComponentProps } from 'svelte';
  let {
    ref = $bindable(null),
    class: className = '',
    ...restProps
  }: Omit<ComponentProps<typeof Separator>, 'class'> & { class?: string } = $props();
</script>

<Separator
  bind:ref
  data-slot="item-separator"
  orientation="horizontal"
  class={cn('my-0', className)}
  {...restProps}
/>
