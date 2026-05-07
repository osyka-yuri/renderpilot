<script lang="ts">
  type SurfaceTone = 'panel' | 'elevated' | 'soft';
  type SurfaceRadius = 'md' | 'lg';
  type SurfaceElement = keyof HTMLElementTagNameMap;

  export let as: SurfaceElement = 'div';
  export let tone: SurfaceTone = 'panel';
  export let radius: SurfaceRadius = 'lg';
  export let shadow = false;
  export let interactive = false;

  let classAttr = '';
  export { classAttr as class };
  export let className = '';
</script>

<svelte:element
  this={as}
  {...$$restProps}
  class={['surface', classAttr, className].filter(Boolean).join(' ')}
  data-tone={tone}
  data-radius={radius}
  data-shadow={shadow ? '' : undefined}
  data-interactive={interactive ? '' : undefined}
>
  <slot />
</svelte:element>

<style>
  .surface {
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
