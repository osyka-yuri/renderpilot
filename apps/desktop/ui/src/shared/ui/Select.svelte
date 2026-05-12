<script module lang="ts">
  export type SelectSize = 'md' | 'sm';

  export type SelectOption = Readonly<{
    value: string;
    label: string;
    disabled?: boolean;
  }>;
</script>

<script lang="ts">
  import { cn, normalizeA11yTextProps } from '@shared/utils';
  import type { HTMLSelectAttributes } from 'svelte/elements';

  type NativeSelectProps = Omit<
    HTMLSelectAttributes,
    | 'class'
    | 'value'
    | 'size'
    | 'disabled'
    | 'required'
    | 'onchange'
    | 'aria-label'
    | 'aria-labelledby'
    | 'aria-describedby'
    | 'title'
  >;

  type SelectProps = NativeSelectProps & {
    options?: readonly SelectOption[];
    value?: string;
    size?: SelectSize;
    disabled?: boolean;
    required?: boolean;
    class?: string;
    title?: string | null;
    'aria-label'?: string | null;
    'aria-labelledby'?: string | null;
    'aria-describedby'?: string | null;
    onValueChange?: ((nextValue: string) => void) | undefined;
  };

  let {
    options = [],
    value = $bindable(''),
    disabled = false,
    required = false,
    size = 'md',
    class: className = '',
    title,
    'aria-label': ariaLabel,
    'aria-labelledby': ariaLabelledBy,
    'aria-describedby': ariaDescribedBy,
    onValueChange,
    ...restProps
  }: SelectProps = $props();

  const selectRootClass = $derived(cn('group relative block w-full', className));

  const selectFieldClass = $derived(
    cn(
      'min-h-8 w-full min-w-0 py-1.5 pr-9 pl-3',
      'rounded-2xl border border-border-control',
      'bg-bg-control text-text-strong',
      'leading-tight',
      'cursor-pointer appearance-none',
      'transition duration-150',
      'disabled:cursor-not-allowed disabled:opacity-45',
      'hover:border-border-strong hover:bg-bg-control-hover',
      'active:bg-bg-control-pressed',
      'focus-visible:border-accent-outline focus-visible:bg-bg-control focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-bg-base focus-visible:outline-none',
      size === 'sm' && 'min-h-8 py-1.5 pr-8 pl-3 text-xs',
    ),
  );

  const a11yText = $derived(
    normalizeA11yTextProps({
      label: ariaLabel,
      labelledBy: ariaLabelledBy,
      describedBy: ariaDescribedBy,
    }),
  );

  const selectTitle = $derived(title ?? a11yText.title);

  function getSelectValue(event: Event): string | null {
    const target = event.currentTarget;

    if (!(target instanceof HTMLSelectElement)) {
      return null;
    }

    return target.value;
  }

  function handleChange(event: Event): void {
    const nextValue = getSelectValue(event);

    if (nextValue === null) {
      return;
    }

    value = nextValue;
    onValueChange?.(nextValue);
  }
</script>

<span class={selectRootClass}>
  <span
    class={cn(
      'pointer-events-none absolute top-1/2 right-3 size-2 -translate-y-2/3',
      'rotate-45 border-r border-b border-text-muted transition duration-150',
      'group-focus-within:border-text-strong',
      'group-hover:border-text-soft',

      disabled && 'opacity-45',
    )}
    aria-hidden="true"
  ></span>
  <select
    {...restProps}
    {disabled}
    {required}
    {value}
    title={selectTitle}
    class={selectFieldClass}
    aria-label={a11yText.ariaLabel}
    aria-labelledby={a11yText.ariaLabelledBy}
    aria-describedby={a11yText.ariaDescribedBy}
    onchange={handleChange}
  >
    {#each options as option (option.value)}
      <option value={option.value} disabled={option.disabled === true}>
        {option.label}
      </option>
    {/each}
  </select>
</span>
