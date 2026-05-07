<script lang="ts">
  type SelectSize = 'md' | 'sm';

  type SelectOption = Readonly<{
    value: string;
    label: string;
    disabled?: boolean;
  }>;

  export let options: readonly SelectOption[] = [];

  export let value = '';
  export let disabled = false;
  export let required = false;

  export let id: string | undefined = undefined;
  export let name: string | undefined = undefined;
  export let title: string | undefined = undefined;

  export let ariaLabel: string | undefined = 'Select option';
  export let ariaLabelledby: string | undefined = undefined;
  export let ariaDescribedby: string | undefined = undefined;

  export let size: SelectSize = 'md';

  export let onValueChange: ((value: string) => void) | undefined = undefined;

  $: resolvedAriaLabel = ariaLabelledby ? undefined : ariaLabel;

  function handleChange(event: Event): void {
    const select = event.currentTarget;

    if (!(select instanceof HTMLSelectElement)) {
      return;
    }

    const nextValue = select.value;

    value = nextValue;
    onValueChange?.(nextValue);
  }
</script>

<span
  class="select-root"
  class:select-root--sm={size === 'sm'}
  class:select-root--disabled={disabled}
>
  <select
    {id}
    {name}
    {title}
    {disabled}
    {required}
    {value}
    class="select-field"
    aria-label={resolvedAriaLabel}
    aria-labelledby={ariaLabelledby}
    aria-describedby={ariaDescribedby}
    onchange={handleChange}
  >
    {#each options as option (option.value)}
      <option value={option.value} disabled={option.disabled}>
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

  .select-field {
    width: 100%;
    min-width: 0;

    min-height: 2rem;
    border: 1px solid var(--border-control);
    border-radius: var(--radius-md);

    background: var(--bg-control);
    color: var(--text-strong);
    color-scheme: var(--native-color-scheme);

    font: inherit;
    line-height: 1.2;

    padding: 0.4rem 2.15rem 0.4rem 0.75rem;

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

  .select-root:not(.select-root--disabled):hover::after {
    border-color: var(--text-soft);
  }

  .select-field:focus-visible {
    background: var(--bg-control);
    border-color: var(--accent-outline);
    box-shadow: var(--shadow-focus);
    outline: none;
  }

  .select-root:focus-within::after {
    border-color: var(--text-strong);
  }
</style>
