<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements';
  import { cn } from '@shared/classnames';
  import type { HeadingLevel, WithElementRef } from '../types';

  let {
    ref = $bindable(null),
    class: className,
    children,
    level,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLHeadingElement>, HTMLHeadingElement> & {
    level: HeadingLevel;
  } = $props();

  const element = $derived(`h${level}` as const);
</script>

<svelte:element
  this={element}
  bind:this={ref}
  data-slot="card-title"
  class={cn('leading-none font-semibold', className)}
  {...restProps}
>
  {@render children?.()}
</svelte:element>
