<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  type Props = HTMLAttributes<HTMLDivElement> & {
    children?: Snippet;
  };

  let { children, class: className = '', ...rest }: Props = $props();
</script>

<div {...rest} class={['setting-row', className]}>
  {@render children?.()}
</div>

<style>
  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4);
    border-bottom: 1px solid var(--border-subtle);
  }

  .setting-row:last-child {
    border-bottom: 0;
  }

  @media (max-width: 720px) {
    .setting-row {
      flex-direction: column;
      align-items: stretch;
      gap: 0.75rem;
    }
  }
</style>
