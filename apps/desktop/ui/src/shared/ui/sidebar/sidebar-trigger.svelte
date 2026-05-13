<script lang="ts">
  import { Button } from '../button/index.js';
  import { cn } from '@shared/classnames';
  import PanelLeftIcon from '@lucide/svelte/icons/panel-left';
  import { useSidebar } from './context.svelte.js';

  let {
    ref = $bindable(null),
    class: className,
    onclick: onclickProp,
    ...restProps
  }: {
    ref?: HTMLElement | null;
    class?: string;
    onclick?: (e: MouseEvent) => void;
    [key: string]: unknown;
  } = $props();

  const sidebar = useSidebar();

  const onclick = (e: MouseEvent) => {
    onclickProp?.(e);
    sidebar.toggle();
  };
</script>

<Button
  bind:ref
  data-sidebar="trigger"
  data-slot="sidebar-trigger"
  variant="ghost"
  size="icon"
  class={cn('size-7', className)}
  type="button"
  {onclick}
  {...restProps as Record<string, unknown>}
>
  <PanelLeftIcon />
  <span class="sr-only">Toggle Sidebar</span>
</Button>
