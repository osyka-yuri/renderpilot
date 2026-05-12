<script lang="ts">
  import { cn } from '@shared/utils';
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  type Props = HTMLAttributes<HTMLDivElement> & {
    align?: 'start' | 'end';
    children?: Snippet;
  };

  const { align = 'start', class: className = '', children, ...rest }: Props = $props();

  const classes = $derived(
    cn(
      'flex flex-wrap content-start items-start gap-2',
      align === 'end' && 'justify-end',
      className,
    ),
  );
</script>

<div {...rest} class={classes}>
  {@render children?.()}
</div>
