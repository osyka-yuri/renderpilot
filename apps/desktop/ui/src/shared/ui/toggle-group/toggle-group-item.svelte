<script lang="ts">
  import { ToggleGroup as ToggleGroupPrimitive } from 'bits-ui';

  import { cn } from '@shared/classnames';

  import { toggleVariants } from '../toggle/toggle.variants';
  import type { ToggleVariants } from '../toggle/toggle.types';

  import { getToggleGroupCtx } from './toggle-group.context';

  type ToggleGroupItemProps = ToggleGroupPrimitive.ItemProps & ToggleVariants;

  const TOGGLE_GROUP_ITEM_CLASS =
    'w-auto min-w-0 shrink-0 px-3 focus:z-10 focus-visible:z-10 data-[spacing=0]:rounded-none data-[spacing=0]:shadow-none data-[spacing=0]:first:rounded-l-md data-[spacing=0]:last:rounded-r-md data-[spacing=0]:data-[variant=outline]:border-l-0 data-[spacing=0]:data-[variant=outline]:first:border-l';

  let {
    ref = $bindable(null),
    value,
    class: className,
    size,
    variant,
    ...itemProps
  }: ToggleGroupItemProps = $props();

  const ctx = getToggleGroupCtx();

  const resolvedVariant = $derived(variant ?? ctx.variant);
  const resolvedSize = $derived(size ?? ctx.size);

  const itemClassName = $derived(
    cn(
      toggleVariants({
        variant: resolvedVariant,
        size: resolvedSize,
      }),
      TOGGLE_GROUP_ITEM_CLASS,
      className,
    ),
  );
</script>

<ToggleGroupPrimitive.Item
  bind:ref
  data-slot="toggle-group-item"
  data-variant={resolvedVariant}
  data-size={resolvedSize}
  data-spacing={ctx.spacing}
  class={itemClassName}
  {value}
  {...itemProps}
/>
