<script lang="ts">
  import { cva } from 'class-variance-authority';
  import { cn } from '@shared/utils';
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  const surfaceVariants = cva(
    'block border border-border-subtle text-text-strong transition duration-150 motion-reduce:transition-none',
    {
      variants: {
        tone: {
          panel: 'bg-bg-card',
          elevated: 'bg-bg-elevated',
          soft: 'bg-bg-soft',
          sunken: 'border-dashed border-border-strong bg-bg-soft',
        },
        radius: {
          md: 'rounded-2xl',
          lg: 'rounded-2xl',
        },
        shadow: {
          true: 'shadow-sm',
          false: '',
        },
        interactive: {
          true: 'focus-within:border-accent-outline focus-within:ring-2 focus-within:ring-accent focus-within:ring-offset-2 focus-within:ring-offset-bg-base hover:border-border-strong hover:bg-bg-card-hover hover:shadow-sm',
          false: '',
        },
      },
      defaultVariants: {
        tone: 'panel',
        radius: 'lg',
        shadow: false,
        interactive: false,
      },
    },
  );

  type SurfaceTone = 'panel' | 'elevated' | 'soft' | 'sunken';
  type SurfaceRadius = 'md' | 'lg';
  type SurfaceElement = keyof HTMLElementTagNameMap;

  type Props = HTMLAttributes<HTMLElement> & {
    as?: SurfaceElement;
    tone?: SurfaceTone;
    radius?: SurfaceRadius;
    shadow?: boolean;
    interactive?: boolean;
    children?: Snippet;
  };

  const {
    as = 'div',
    tone = 'panel',
    radius = 'lg',
    shadow = false,
    interactive = false,
    class: className = '',
    children,
    ...rest
  }: Props = $props();

  const classes = $derived(cn(surfaceVariants({ tone, radius, shadow, interactive }), className));
</script>

<svelte:element this={as} {...rest} class={classes}>
  {@render children?.()}
</svelte:element>
