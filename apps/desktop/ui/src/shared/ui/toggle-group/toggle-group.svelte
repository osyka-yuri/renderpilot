<script lang="ts">
  import { ToggleGroup as ToggleGroupPrimitive } from 'bits-ui';

  import { cn } from '@shared/classnames';

  import type { ToggleGroupSize, ToggleGroupVariant } from './toggle-group.context';
  import { setToggleGroupCtx } from './toggle-group.context';

  type ToggleGroupProps = ToggleGroupPrimitive.RootProps & {
    variant?: ToggleGroupVariant;
    size?: ToggleGroupSize;
    spacing?: number;
  };

  const DEFAULT_VARIANT = 'default' satisfies ToggleGroupVariant;
  const DEFAULT_SIZE = 'default' satisfies ToggleGroupSize;
  const DEFAULT_SPACING = 0;

  const ROOT_CLASS =
    'group/toggle-group flex w-fit items-center gap-[--spacing(var(--gap))] rounded-md data-[spacing=default]:data-[variant=outline]:shadow-xs';

  let {
    ref = $bindable(null),
    value = $bindable(),
    class: className,
    variant = DEFAULT_VARIANT,
    size = DEFAULT_SIZE,
    spacing = DEFAULT_SPACING,
    ...rootProps
  }: ToggleGroupProps = $props();

  setToggleGroupCtx({
    get variant() {
      return variant;
    },
    get size() {
      return size;
    },
    get spacing() {
      return spacing;
    },
  });

  const rootClassName = $derived(cn(ROOT_CLASS, className));
  const rootStyle = $derived(`--gap: ${spacing}`);
</script>

<ToggleGroupPrimitive.Root
  bind:ref
  bind:value={value as never}
  data-slot="toggle-group"
  data-variant={variant}
  data-size={size}
  data-spacing={spacing}
  style={rootStyle}
  class={rootClassName}
  {...rootProps}
/>
