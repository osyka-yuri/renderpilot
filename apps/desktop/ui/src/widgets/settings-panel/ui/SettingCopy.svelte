<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  type Props = HTMLAttributes<HTMLDivElement> & {
    children?: Snippet;
  };

  let { children, class: className = '', ...rest }: Props = $props();
</script>

<div {...rest} class={['setting-copy', className]}>
  {@render children?.()}
</div>

<style>
  .setting-copy {
    display: grid;
    flex: 1;
    min-width: 0;
    gap: var(--space-1);
  }

  .setting-copy :global(.row-title) {
    margin: 0;
    color: var(--text-strong);
    font-size: 0.95rem;
    font-weight: 600;
  }

  .setting-copy :global(.row-copy) {
    margin: 0;
    font-size: 0.84rem;
    line-height: 1.45;
  }
</style>
