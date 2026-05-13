<script lang="ts">
  import { Toggle as TogglePrimitive } from 'bits-ui';

  import { cn } from '@shared/classnames';

  import type { ToggleSize, ToggleVariant } from './toggle.types';
  import { toggleVariants } from './toggle.variants';

  type ToggleProps = TogglePrimitive.RootProps & {
    variant?: ToggleVariant;
    size?: ToggleSize;
  };

  const DEFAULT_VARIANT = 'default' satisfies ToggleVariant;
  const DEFAULT_SIZE = 'default' satisfies ToggleSize;

  let {
    ref = $bindable(null),
    pressed = $bindable(false),
    class: className,
    variant = DEFAULT_VARIANT,
    size = DEFAULT_SIZE,
    ...rootProps
  }: ToggleProps = $props();

  const rootClassName = $derived(cn(toggleVariants({ variant, size }), className));
</script>

<TogglePrimitive.Root
  bind:ref
  bind:pressed
  data-slot="toggle"
  class={rootClassName}
  {...rootProps}
/>
