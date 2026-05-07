<script context="module" lang="ts">
  export type BadgeTone = 'neutral' | 'muted' | 'success' | 'warning' | 'danger';
  export type BadgeSurface = 'soft' | 'outline';
  export type BadgeSize = 'sm' | 'md';
</script>

<script lang="ts">
  export let tone: BadgeTone = 'neutral';
  export let surface: BadgeSurface = 'soft';
  export let size: BadgeSize = 'sm';
  export let pill = false;
  export let dot = false;

  let className = '';
  export { className as class };

  $: classes = ['badge', pill && 'badge--pill', className].filter(Boolean).join(' ');
</script>

<span {...$$restProps} class={classes} data-tone={tone} data-surface={surface} data-size={size}>
  {#if dot}
    <span class="badge__dot" aria-hidden="true"></span>
  {/if}

  <slot />
</span>

<style>
  .badge {
    --badge-color: var(--text-strong);
    --badge-background: color-mix(in srgb, var(--bg-control) 80%, var(--bg-soft) 20%);
    --badge-border-color: var(--border-subtle);
    --badge-radius: var(--radius-sm);

    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;

    width: fit-content;
    max-width: 100%;

    border: 1px solid var(--badge-border-color);
    border-radius: var(--badge-radius);
    background: var(--badge-background);
    color: var(--badge-color);

    font-weight: 600;
    line-height: 1;
    letter-spacing: 0.01em;
    white-space: nowrap;
    vertical-align: middle;
  }

  .badge--pill {
    --badge-radius: 999px;
  }

  .badge[data-size='sm'] {
    min-height: 1.375rem;
    padding: 0.22rem 0.5rem;
    font-size: 0.75rem;
  }

  .badge[data-size='md'] {
    min-height: 1.75rem;
    padding: 0.34rem 0.68rem;
    font-size: 0.8125rem;
  }

  .badge[data-surface='outline'] {
    --badge-background: color-mix(in srgb, var(--bg-card) 70%, transparent);
  }

  .badge[data-tone='neutral'] {
    --badge-color: var(--text-strong);
  }

  .badge[data-tone='muted'] {
    --badge-color: var(--text-muted);
  }

  .badge[data-tone='success'] {
    --badge-color: var(--success);
  }

  .badge[data-tone='warning'] {
    --badge-color: var(--warning);
  }

  .badge[data-tone='danger'] {
    --badge-color: var(--danger);
  }

  .badge[data-surface='soft'][data-tone='success'] {
    --badge-background: color-mix(in srgb, var(--success) 13%, var(--bg-panel) 87%);
    --badge-border-color: color-mix(in srgb, var(--success) 24%, var(--border-subtle));
  }

  .badge[data-surface='soft'][data-tone='warning'] {
    --badge-background: color-mix(in srgb, var(--warning) 13%, var(--bg-panel) 87%);
    --badge-border-color: color-mix(in srgb, var(--warning) 24%, var(--border-subtle));
  }

  .badge[data-surface='soft'][data-tone='danger'] {
    --badge-background: color-mix(in srgb, var(--danger) 13%, var(--bg-panel) 87%);
    --badge-border-color: color-mix(in srgb, var(--danger) 24%, var(--border-subtle));
  }

  .badge__dot {
    width: 0.42rem;
    height: 0.42rem;
    flex: 0 0 auto;
    border-radius: 999px;
    background: currentColor;
    opacity: 0.72;
  }
</style>
