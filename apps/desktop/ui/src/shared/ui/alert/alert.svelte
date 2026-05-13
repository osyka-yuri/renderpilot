<script lang="ts">
  import { cn } from '@shared/classnames';
  import { alertVariants } from './alert.variants';
  import type { AlertProps } from './alert.types';

  let {
    ref = $bindable(null),
    class: className,
    variant = 'default',
    children,
    ...restProps
  }: AlertProps = $props();
</script>

<div
  bind:this={ref}
  data-slot="alert"
  role="alert"
  class={cn(alertVariants({ variant }), className)}
  {...restProps}
>
  {@render children?.()}
</div>
