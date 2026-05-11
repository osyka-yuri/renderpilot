<script module lang="ts">
  export type SelectSize = 'md' | 'sm';

  export type SelectOption = Readonly<{
    value: string;
    label: string;
    disabled?: boolean;
  }>;
</script>

<script lang="ts">
  import { normalizeA11yTextProps } from '@shared/utils';
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

  const selectRootClass = $derived([
    'select-root',
    size === 'sm' && 'select-root--sm',
    disabled && 'select-root--disabled',
    className,
  ]);

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
  <select
    {...restProps}
    {disabled}
    {required}
    {value}
    title={selectTitle}
    class="select-field"
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

<style>
  .select-root {
    position: relative;
    display: block;
    width: 100%;
  }

  .select-root::after {
    content: '';
    position: absolute;
    top: 50%;
    right: 0.78rem;

    width: 0.5rem;
    height: 0.5rem;

    border-right: 1.5px solid var(--text-muted);
    border-bottom: 1.5px solid var(--text-muted);

    pointer-events: none;
    transform: translateY(-65%) rotate(45deg);
    transition:
      border-color 160ms ease,
      opacity 160ms ease;
  }

  .select-root--disabled::after {
    opacity: 0.45;
  }

  .select-root:not(.select-root--disabled):hover::after {
    border-color: var(--text-soft);
  }

  .select-root:focus-within::after {
    border-color: var(--text-strong);
  }

  .select-field {
    width: 100%;
    min-width: 0;
    min-height: 2rem;

    padding: 0.4rem 2.15rem 0.4rem 0.75rem;

    border: 1px solid var(--border-control);
    border-radius: var(--radius-md);

    background: var(--bg-control);
    color: var(--text-strong);
    color-scheme: var(--native-color-scheme);

    font: inherit;
    line-height: 1.2;

    appearance: none;
    cursor: pointer;

    transition:
      border-color 160ms ease,
      background 160ms ease,
      box-shadow 160ms ease,
      opacity 160ms ease;
  }

  .select-field option {
    background: var(--bg-panel-strong);
    color: var(--text-strong);
  }

  .select-root--sm .select-field {
    min-height: 1.875rem;
    padding: 0.34rem 2rem 0.34rem 0.68rem;
    font-size: 0.8125rem;
  }

  .select-field:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .select-field:not(:disabled):hover {
    background: var(--bg-control-hover);
    border-color: var(--border-strong);
  }

  .select-field:not(:disabled):active {
    background: var(--bg-control-pressed);
  }

  .select-field:focus-visible {
    background: var(--bg-control);
    border-color: var(--accent-outline);
    box-shadow: var(--shadow-focus);
    outline: none;
  }
</style>
