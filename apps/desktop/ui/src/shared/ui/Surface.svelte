<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

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

  let {
    as = 'div',
    tone = 'panel',
    radius = 'lg',
    shadow = false,
    interactive = false,
    class: className = '',
    children,
    ...rest
  }: Props = $props();

  const classes = $derived(['surface', className]);
</script>

<svelte:element
  this={as}
  {...rest}
  class={classes}
  data-tone={tone}
  data-radius={radius}
  data-shadow={shadow ? '' : undefined}
  data-interactive={interactive ? '' : undefined}
>
  {@render children?.()}
</svelte:element>

<style>
  .surface {
    display: block;
    background: var(--bg-card);
    border: 1px solid var(--border-subtle);
    color: var(--text-strong);
  }

  .surface[data-tone='panel'] {
    background: var(--bg-card);
  }

  .surface[data-tone='elevated'] {
    background: var(--bg-elevated);
  }

  .surface[data-tone='soft'] {
    background: color-mix(in srgb, var(--bg-soft) 58%, var(--bg-panel) 42%);
  }

  .surface[data-tone='sunken'] {
    background: var(--bg-soft);
    border-style: dashed;
    border-color: var(--border-strong);
  }

  .surface[data-radius='md'] {
    border-radius: var(--radius-lg);
  }

  .surface[data-radius='lg'] {
    border-radius: var(--radius-xl);
  }

  .surface[data-shadow] {
    box-shadow: var(--shadow-card);
  }

  .surface[data-interactive] {
    transition:
      border-color 140ms ease,
      background-color 140ms ease,
      box-shadow 140ms ease;
  }

  .surface[data-interactive]:hover {
    background: var(--bg-card-hover);
    border-color: var(--border-strong);
    box-shadow: var(--shadow-card);
  }

  .surface[data-interactive]:focus-within {
    border-color: var(--accent-outline);
    box-shadow: var(--shadow-focus, var(--shadow-card));
  }

  @media (prefers-reduced-motion: reduce) {
    .surface[data-interactive] {
      transition: none;
    }
  }
</style>
