<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  import { cn } from '@shared/classnames';

  type Props = Omit<HTMLAttributes<HTMLDivElement>, 'children'> & {
    label: string;
    children?: Snippet;
  };

  let { label, children, class: className, ...restProps }: Props = $props();

  const containerClass = $derived(cn('inline-flex flex-wrap items-center gap-2', className));
</script>

<div {...restProps} class={containerClass}>
  <span class="text-sm font-medium text-muted-foreground">{label}</span>
  {@render children?.()}
</div>
