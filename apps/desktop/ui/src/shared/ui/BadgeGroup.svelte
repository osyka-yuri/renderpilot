<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  type Props = HTMLAttributes<HTMLDivElement> & {
    align?: 'start' | 'end';
    children?: Snippet;
  };

  let { align = 'start', class: className = '', children, ...rest }: Props = $props();

  const classes = $derived(['badge-group', align === 'end' && 'badge-group--end', className]);
</script>

<div {...rest} class={classes}>
  {@render children?.()}
</div>

<style>
  .badge-group {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: flex-start;
    align-content: flex-start;
    justify-content: flex-start;
  }

  .badge-group--end {
    justify-content: flex-end;
  }
</style>
