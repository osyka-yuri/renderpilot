<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  type Props = HTMLAttributes<HTMLParagraphElement> & {
    children?: Snippet;
  };

  let { children, class: className = '', ...rest }: Props = $props();
</script>

<p {...rest} class={['setting-label', className]}>
  {@render children?.()}
</p>

<style>
  .setting-label {
    margin: 0;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
</style>
