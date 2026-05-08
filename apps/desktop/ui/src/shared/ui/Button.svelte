<script lang="ts">
  import { cx } from '@shared/utils/cx';
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  type ButtonVariant = 'primary' | 'secondary' | 'ghost';
  type ButtonSize = 'md' | 'sm';
  type ButtonType = 'button' | 'submit' | 'reset';
  type AriaPressed = boolean | 'mixed';

  type NativeButtonProps = Omit<
    HTMLButtonAttributes,
    'class' | 'disabled' | 'type' | 'aria-label' | 'aria-pressed' | 'aria-busy'
  >;

  type ButtonProps = NativeButtonProps & {
    children?: Snippet;

    variant?: ButtonVariant;
    size?: ButtonSize;
    type?: ButtonType;

    disabled?: boolean;
    active?: boolean;
    loading?: boolean;
    fullWidth?: boolean;
    iconOnly?: boolean;

    class?: string;
    title?: string | null;

    'aria-label'?: string;
    'aria-pressed'?: AriaPressed;
  };

  let {
    children,

    variant = 'secondary',
    size = 'md',
    type = 'button',

    disabled = false,
    active = false,
    loading = false,
    fullWidth = false,
    iconOnly = false,

    class: className = '',
    title = null,

    'aria-label': ariaLabel,
    'aria-pressed': ariaPressed,

    ...restProps
  }: ButtonProps = $props();

  const isDisabled = $derived(disabled || loading);

  const normalizedTitle = $derived(title ?? undefined);

  const resolvedAriaLabel = $derived(ariaLabel ?? getFallbackIconLabel(iconOnly, title));

  const buttonClass = $derived(
    cx(
      'ui-button',
      `ui-button--${variant}`,
      `ui-button--${size}`,
      active && 'is-active',
      loading && 'is-loading',
      fullWidth && 'is-full-width',
      iconOnly && 'is-icon-only',
      className,
    ),
  );

  function getFallbackIconLabel(iconOnly: boolean, title: string | null): string | undefined {
    if (!iconOnly) return undefined;

    const label = title?.trim();
    return label ?? undefined;
  }
</script>

<button
  {...restProps}
  {type}
  title={normalizedTitle}
  disabled={isDisabled}
  class={buttonClass}
  aria-label={resolvedAriaLabel}
  aria-pressed={ariaPressed}
  aria-busy={loading ? true : undefined}
>
  {@render children?.()}
</button>

<style>
  .ui-button {
    --button-height: 2rem;
    --button-padding-x: 0.85rem;
    --button-padding-y: 0.42rem;
    --button-radius: var(--radius-md);
    --button-gap: 0.4rem;
    --button-icon-size: 1rem;

    box-sizing: border-box;
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    gap: var(--button-gap);

    min-height: var(--button-height);
    padding: var(--button-padding-y) var(--button-padding-x);

    appearance: none;
    border: 1px solid var(--border-control);
    border-radius: var(--button-radius);
    background: var(--bg-control);
    color: var(--text-strong);

    font: inherit;
    font-weight: 600;
    font-size: 0.875rem;
    line-height: 1.15;
    white-space: nowrap;
    user-select: none;
    cursor: pointer;

    transition:
      background 140ms ease,
      border-color 140ms ease,
      color 140ms ease,
      box-shadow 140ms ease,
      opacity 140ms ease,
      transform 80ms ease;
  }

  .ui-button--sm {
    --button-height: 1.875rem;
    --button-padding-x: 0.72rem;
    --button-padding-y: 0.34rem;

    font-size: 0.8125rem;
  }

  .ui-button--primary {
    border-color: transparent;
    background: var(--accent);
    color: var(--accent-contrast);
    box-shadow: inset 0 1px 0 color-mix(in srgb, white 28%, transparent);
  }

  .ui-button--secondary {
    background: var(--bg-control);
    color: var(--text-strong);
  }

  .ui-button--ghost {
    border-color: transparent;
    background: transparent;
    color: var(--text-soft);
  }

  .ui-button--secondary.is-active,
  .ui-button--ghost.is-active {
    border-color: var(--accent-outline);
    background: var(--accent-soft);
    color: var(--accent-strong);
  }

  .ui-button--primary.is-active,
  .ui-button--primary:not(:disabled):hover {
    background: var(--accent-strong);
  }

  .ui-button--primary:not(:disabled):active {
    background: var(--accent-pressed);
  }

  .ui-button--secondary:not(:disabled):hover {
    border-color: var(--border-strong);
    background: var(--bg-control-hover);
  }

  .ui-button--secondary:not(:disabled):active {
    background: var(--bg-control-pressed);
    transform: translateY(1px);
  }

  .ui-button--ghost:not(:disabled):hover {
    background: var(--bg-soft);
  }

  .ui-button--ghost:not(:disabled):active {
    background: var(--bg-control-pressed);
    transform: translateY(1px);
  }

  .ui-button:focus-visible {
    outline: none;
    box-shadow: var(--shadow-focus);
  }

  .ui-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .ui-button.is-loading {
    position: relative;
  }

  .ui-button.is-loading::before {
    content: '';
    width: 0.75rem;
    height: 0.75rem;
    border: 1.5px solid currentColor;
    border-block-start-color: transparent;
    border-radius: 999px;
    opacity: 0.75;
    animation: button-spin 900ms linear infinite;
  }

  .ui-button.is-full-width {
    width: 100%;
  }

  .ui-button.is-icon-only {
    width: var(--button-height);
    height: var(--button-height);
    min-height: var(--button-height);
    padding: 0;
  }

  .ui-button.is-icon-only :global(svg) {
    width: var(--button-icon-size);
    height: var(--button-icon-size);
    flex-shrink: 0;
  }

  @keyframes button-spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .ui-button {
      transition: none;
    }

    .ui-button.is-loading::before {
      animation: none;
    }
  }
</style>
