<script lang="ts">
  import { cn, normalizeA11yTextProps } from '@shared/utils';
  import type { HTMLInputAttributes } from 'svelte/elements';

  type InputSize = 'md' | 'sm';
  type InputType = NonNullable<HTMLInputAttributes['type']>;
  type InputValueChangeHandler = (nextValue: string) => void;

  type NativeInputProps = Omit<
    HTMLInputAttributes,
    | 'class'
    | 'size'
    | 'value'
    | 'type'
    | 'readonly'
    | 'oninput'
    | 'aria-label'
    | 'aria-labelledby'
    | 'aria-describedby'
  >;

  type InputProps = NativeInputProps & {
    value?: string;
    type?: InputType;
    size?: InputSize;
    readonly?: boolean;
    class?: string;
    'aria-label'?: string | null;
    'aria-labelledby'?: string | null;
    'aria-describedby'?: string | null;
    onValueChange?: InputValueChangeHandler;
  };

  let {
    value = $bindable(''),
    type = 'text',
    size = 'md',
    readonly: isReadonly = false,
    class: className = '',
    'aria-label': ariaLabel,
    'aria-labelledby': ariaLabelledBy,
    'aria-describedby': ariaDescribedBy,
    onValueChange,
    ...restProps
  }: InputProps = $props();

  const inputClass = $derived(
    cn(
      'min-h-8 w-full rounded-2xl border border-border-control bg-bg-control text-text-strong',
      'px-3 py-1.5 leading-tight',
      'transition duration-160',
      'placeholder:text-text-muted',
      'hover:border-border-strong hover:bg-bg-control-hover',
      'focus-visible:border-accent-outline focus-visible:bg-bg-control focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-bg-base focus-visible:outline-none',
      'disabled:cursor-not-allowed disabled:opacity-50',
      'read-only:cursor-default',
      size === 'sm' && 'min-h-7.5 px-2.5 py-1.5 text-xs',
      className,
    ),
  );

  const a11yText = $derived(
    normalizeA11yTextProps({
      label: ariaLabel,
      labelledBy: ariaLabelledBy,
      describedBy: ariaDescribedBy,
    }),
  );

  function getInputElement(event: Event): HTMLInputElement | null {
    const element = event.currentTarget;
    return element instanceof HTMLInputElement ? element : null;
  }

  function handleInput(event: Event): void {
    const input = getInputElement(event);

    if (input === null) {
      return;
    }

    const nextValue = input.value;

    value = nextValue;
    onValueChange?.(nextValue);
  }
</script>

<input
  {...restProps}
  {type}
  readonly={isReadonly}
  {value}
  class={inputClass}
  aria-label={a11yText.ariaLabel}
  aria-labelledby={a11yText.ariaLabelledBy}
  aria-describedby={a11yText.ariaDescribedBy}
  oninput={handleInput}
/>
