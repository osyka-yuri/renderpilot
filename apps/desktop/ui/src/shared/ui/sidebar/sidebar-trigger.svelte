<script lang="ts">
  import { Button } from '../button/index.js';
  import { cn } from '@shared/classnames';
  import PanelLeftIcon from '@lucide/svelte/icons/panel-left';
  import type { ComponentProps } from 'svelte';
  import { useSidebar } from './context.svelte.js';

  type Props = Omit<
    ComponentProps<typeof Button>,
    'children' | 'class' | 'href' | 'onclick' | 'size' | 'type' | 'variant'
  > & {
    class?: string;
    label: string;
    onclick?: (event: MouseEvent) => void;
  };

  let {
    ref = $bindable(null),
    class: className,
    onclick: onclickProp,
    label,
    ...restProps
  }: Props = $props();

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
  {...restProps}
>
  <PanelLeftIcon class="rtl:rotate-180" aria-hidden="true" />
  <span class="sr-only">{label}</span>
</Button>
