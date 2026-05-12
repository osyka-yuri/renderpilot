<script lang="ts">
  import { cn } from '@shared/utils';
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  type Props = HTMLAttributes<HTMLParagraphElement> & {
    children?: Snippet;
  };

  const { children, class: className = '', ...rest }: Props = $props();
</script>

<p {...rest} class={cn('text-xs tracking-widest text-text-subtle uppercase', className)}>
  {@render children?.()}
</p>
