<script lang="ts">
  import { RadioGroup as RadioGroupPrimitive } from 'bits-ui';
  import { cn } from '@shared/classnames';

  import type { SegmentedControlSize } from './segmented-control.context';
  import { setSegmentedControlCtx } from './segmented-control.context';

  type SegmentedControlProps = Omit<RadioGroupPrimitive.RootProps, 'orientation'> & {
    size?: SegmentedControlSize;
  };

  const DEFAULT_SIZE = 'default' satisfies SegmentedControlSize;

  const SIZE_CLASSES: Record<SegmentedControlSize, string> = {
    default: 'h-9 p-1',
    sm: 'h-8 gap-0.5 p-0.5',
  };

  let {
    ref = $bindable(null),
    value = $bindable(''),
    class: className,
    size = DEFAULT_SIZE,
    ...rootProps
  }: SegmentedControlProps = $props();

  setSegmentedControlCtx({
    get size() {
      return size;
    },
  });

  const rootClassName = $derived(
    cn(
      'inline-flex w-fit items-center gap-1 rounded-lg bg-muted text-muted-foreground',
      SIZE_CLASSES[size],
      className,
    ),
  );
</script>

<RadioGroupPrimitive.Root
  bind:ref
  bind:value
  orientation="horizontal"
  data-slot="segmented-control"
  data-size={size}
  class={rootClassName}
  {...rootProps}
/>
