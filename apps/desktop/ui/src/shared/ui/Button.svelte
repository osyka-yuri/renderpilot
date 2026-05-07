<script lang="ts">
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

  function cx(...values: ClassValue[]): string {
    return values.filter(isNonEmptyString).join(' ');
  }

  function isNonEmptyString(value: ClassValue): value is string {
    return typeof value === 'string' && value.trim().length > 0;
  }

  type ClassValue = string | false | null | undefined;
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
