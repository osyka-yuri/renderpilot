<script lang="ts">
  import { cx } from '@shared/utils/cx';
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  type NativeSwitchProps = Omit<
    HTMLButtonAttributes,
    | 'class'
    | 'disabled'
    | 'type'
    | 'role'
    | 'title'
    | 'aria-checked'
    | 'aria-label'
    | 'aria-labelledby'
    | 'aria-describedby'
  >;

  type SwitchProps = NativeSwitchProps & {
    children?: Snippet;

    checked?: boolean;
    disabled?: boolean;
    stackOnMobile?: boolean;

    class?: string;
    title?: string | null;

    'aria-label'?: string | null;
    'aria-labelledby'?: string | null;
    'aria-describedby'?: string | null;
  };

  let {
    children,

    checked = false,
    disabled = false,
    stackOnMobile = true,

    class: className = '',
    title = null,

    'aria-label': ariaLabel,
    'aria-labelledby': ariaLabelledBy,
    'aria-describedby': ariaDescribedBy,

    ...restProps
  }: SwitchProps = $props();

  const ariaChecked = $derived(checked ? 'true' : 'false');

  const normalizedTitle = $derived(trimmedOrUndefined(title));
  const normalizedAriaLabel = $derived(trimmedOrUndefined(ariaLabel));
  const normalizedAriaLabelledBy = $derived(trimmedOrUndefined(ariaLabelledBy));
  const normalizedAriaDescribedBy = $derived(trimmedOrUndefined(ariaDescribedBy));

  const stackOnMobileAttribute = $derived(stackOnMobile ? 'true' : undefined);

  const switchClass = $derived(cx('switch-root', className));

  function trimmedOrUndefined(value: string | null | undefined): string | undefined {
    const trimmed = value?.trim();
    return trimmed ?? undefined;
  }
</script>

<button
  {...restProps}
  type="button"
  role="switch"
  aria-checked={ariaChecked}
  aria-label={normalizedAriaLabel}
  aria-labelledby={normalizedAriaLabelledBy}
  aria-describedby={normalizedAriaDescribedBy}
  title={normalizedTitle}
  {disabled}
  class={switchClass}
  data-stack-on-mobile={stackOnMobileAttribute}
>
  <span class="switch-copy">
    {@render children?.()}
  </span>

  <span class="switch-track" aria-hidden="true">
    <span class="switch-thumb"></span>
  </span>
</button>

<style>
  .switch-root,
  .switch-root * {
    box-sizing: border-box;
  }

  .switch-root {
    --switch-width: 2.5rem;
    --switch-height: 1.25rem;
    --switch-padding: 0.125rem;
    --switch-thumb-size: 0.875rem;

    --switch-track-background: var(--bg-control);
    --switch-track-border-color: var(--border-control);
    --switch-thumb-background: var(--text-soft);
    --switch-thumb-position: var(--switch-padding);

    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;

    inline-size: 100%;
    min-inline-size: 0;
    margin: 0;
    padding: 0;
    border: 0;

    appearance: none;
    background: transparent;

    font: inherit;
    color: inherit;
    text-align: start;

    cursor: pointer;
    user-select: none;
    touch-action: manipulation;
  }

  .switch-copy {
    min-inline-size: 0;
    flex: 1 1 auto;
  }

  .switch-track {
    position: relative;
    flex: 0 0 auto;

    inline-size: var(--switch-width);
    block-size: var(--switch-height);
    border: 1px solid var(--switch-track-border-color);
    border-radius: 999px;

    background-color: var(--switch-track-background);

    transition:
      background-color 140ms ease,
      border-color 140ms ease,
      box-shadow 140ms ease;
  }

  .switch-thumb {
    position: absolute;
    inset-block-start: 50%;
    inset-inline-start: var(--switch-thumb-position);

    inline-size: var(--switch-thumb-size);
    block-size: var(--switch-thumb-size);
    border-radius: 999px;

    background-color: var(--switch-thumb-background);
    transform: translateY(-50%);

    pointer-events: none;

    transition:
      inset-inline-start 140ms ease,
      background-color 140ms ease;
  }

  .switch-root[aria-checked='true'] {
    --switch-track-background: var(--accent);
    --switch-track-border-color: transparent;
    --switch-thumb-background: var(--accent-contrast);
    --switch-thumb-position: calc(100% - var(--switch-padding) - var(--switch-thumb-size));
  }

  .switch-root:focus-visible {
    outline: none;
  }

  .switch-root:focus-visible .switch-track {
    box-shadow: var(--shadow-focus);
  }

  @media (hover: hover) {
    .switch-root:not(:disabled):not([aria-checked='true']):hover {
      --switch-track-background: var(--bg-control-hover);
    }

    .switch-root:not(:disabled)[aria-checked='true']:hover {
      --switch-track-background: var(--accent-strong);
    }
  }

  .switch-root:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  @media (max-width: 720px) {
    .switch-root[data-stack-on-mobile='true'] {
      flex-direction: column;
      align-items: stretch;
      gap: 0.75rem;
    }

    .switch-root[data-stack-on-mobile='true'] .switch-track {
      align-self: flex-start;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .switch-track,
    .switch-thumb {
      transition: none;
    }
  }

  @media (forced-colors: active) {
    .switch-track {
      border-color: CanvasText;
    }

    .switch-root:focus-visible .switch-track {
      outline: 2px solid Highlight;
      outline-offset: 2px;
      box-shadow: none;
    }
  }
</style>
