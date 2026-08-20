<script lang="ts" module>
  import { tv, type VariantProps } from 'tailwind-variants';

  export const sidebarMenuButtonVariants = tv({
    base: 'peer/menu-button flex w-full items-center gap-2 overflow-clip rounded-md p-2 text-start text-sm ring-sidebar-ring outline-hidden transition-[width,height,padding] group-has-data-[sidebar=menu-action]/menu-item:pe-8 group-data-[collapsible=icon]:size-8! group-data-[collapsible=icon]:p-2! hover:bg-sidebar-accent hover:text-sidebar-accent-foreground focus-visible:ring-2 active:bg-sidebar-accent active:text-sidebar-accent-foreground disabled:pointer-events-none disabled:opacity-50 aria-disabled:pointer-events-none aria-disabled:opacity-50 data-[active=true]:bg-sidebar-accent data-[active=true]:font-medium data-[active=true]:text-sidebar-accent-foreground data-[state=open]:hover:bg-sidebar-accent data-[state=open]:hover:text-sidebar-accent-foreground [&>span:last-child]:truncate [&>svg]:size-4 [&>svg]:shrink-0',
    variants: {
      variant: {
        default: 'hover:bg-sidebar-accent hover:text-sidebar-accent-foreground',
        outline:
          'bg-background shadow-[0_0_0_1px_var(--sidebar-border)] hover:bg-sidebar-accent hover:text-sidebar-accent-foreground hover:shadow-[0_0_0_1px_var(--sidebar-accent)]',
      },
      size: {
        default: 'h-8 text-sm',
        sm: 'h-7 text-xs',
        lg: 'h-12 text-sm group-data-[collapsible=icon]:p-0!',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  });

  export type SidebarMenuButtonVariant = VariantProps<typeof sidebarMenuButtonVariants>['variant'];
  export type SidebarMenuButtonSize = VariantProps<typeof sidebarMenuButtonVariants>['size'];
</script>

<script lang="ts">
  import { mergeProps } from 'bits-ui';
  import type { ComponentProps, Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  import { cn } from '@shared/classnames';
  import type { WithElementRef, WithoutChildrenOrChild } from '../types';

  import { useSidebar } from './context.svelte.js';
  import * as Tooltip from '../tooltip/index.js';

  type SidebarMenuButtonChildProps = {
    props: Record<string, unknown>;
  };

  type SidebarMenuButtonProps = WithElementRef<HTMLButtonAttributes, HTMLButtonElement> & {
    isActive?: boolean;
    variant?: SidebarMenuButtonVariant;
    size?: SidebarMenuButtonSize;
    tooltipContent?: Snippet | string;
    tooltipContentProps?: WithoutChildrenOrChild<ComponentProps<typeof Tooltip.Content>>;
    child?: Snippet<[SidebarMenuButtonChildProps]>;
  };

  let {
    ref = $bindable(null),
    class: className,
    children,
    child: buttonChild,
    variant = 'default',
    size = 'default',
    isActive = false,
    tooltipContent,
    tooltipContentProps,
    ...restProps
  }: SidebarMenuButtonProps = $props();

  const sidebar = useSidebar();

  const buttonProps = $derived({
    ...restProps,
    class: cn(sidebarMenuButtonVariants({ variant, size }), className),
    'data-slot': 'sidebar-menu-button',
    'data-sidebar': 'menu-button',
    'data-size': size,
    'data-active': isActive,
  });
</script>

{#if !tooltipContent}
  {#if buttonChild}
    {@render buttonChild({ props: buttonProps })}
  {:else}
    <button bind:this={ref} type="button" {...buttonProps}>
      {@render children?.()}
    </button>
  {/if}
{:else}
  <Tooltip.Root>
    <Tooltip.Trigger>
      {#snippet child({ props }: SidebarMenuButtonChildProps)}
        {@const mergedProps = mergeProps(buttonProps, props)}

        {#if buttonChild}
          {@render buttonChild({ props: mergedProps })}
        {:else}
          <button bind:this={ref} type="button" {...mergedProps}>
            {@render children?.()}
          </button>
        {/if}
      {/snippet}
    </Tooltip.Trigger>

    <Tooltip.Content
      side="right"
      align="center"
      hidden={sidebar.state !== 'collapsed' || sidebar.isMobile}
      {...tooltipContentProps}
    >
      {#if typeof tooltipContent === 'string'}
        {tooltipContent}
      {:else if tooltipContent}
        {@render tooltipContent()}
      {/if}
    </Tooltip.Content>
  </Tooltip.Root>
{/if}
