<script lang="ts">
  import { normalizeA11yTextProps } from '@shared/utils/a11y';
  import { cx } from '@shared/utils/cx';
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

  const inputClass = $derived(cx('ui-input', size === 'sm' && 'ui-input--sm', className));

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

<style>
  .ui-input {
    width: 100%;
    min-height: 2rem;
    border: 1px solid var(--border-control);
    border-radius: var(--radius-md);
    background: var(--bg-control);
    color: var(--text-strong);
    font: inherit;
    line-height: 1.2;
    padding: 0.4rem 0.75rem;
    transition:
      border-color 160ms ease,
      background 160ms ease,
      box-shadow 160ms ease,
      opacity 160ms ease;
  }

  .ui-input::placeholder {
    color: var(--text-muted);
  }

  .ui-input:hover:not(:disabled):not(:read-only) {
    background: var(--bg-control-hover);
    border-color: var(--border-strong);
  }

  .ui-input:focus-visible {
    background: var(--bg-control);
    border-color: var(--accent-outline);
    box-shadow: var(--shadow-focus);
    outline: none;
  }

  .ui-input:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .ui-input:read-only {
    cursor: default;
  }

  .ui-input--sm {
    min-height: 1.875rem;
    padding: 0.34rem 0.68rem;
    font-size: 0.8125rem;
  }
</style>
