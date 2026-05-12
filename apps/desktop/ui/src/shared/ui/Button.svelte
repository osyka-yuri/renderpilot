<script lang="ts">
  import { cva } from 'class-variance-authority';
  import { cn } from '@shared/utils';
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  const buttonVariants = cva(
    'inline-flex min-h-8 shrink-0 items-center justify-center gap-1.5 rounded-2xl border border-border-control bg-bg-control px-3.5 py-1.5 text-sm/tight font-semibold whitespace-nowrap text-text-strong transition duration-150 ease-out select-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-bg-base focus-visible:outline-none motion-reduce:transition-none',
    {
      variants: {
        variant: {
          primary: 'border-transparent bg-accent text-accent-contrast',
          secondary: '',
          ghost: 'border-transparent bg-transparent text-text-soft',
          danger: 'border-transparent bg-danger text-white',
        },
        size: {
          md: '',
          sm: 'min-h-8 px-3 py-1.5 text-xs',
        },
        active: {
          true: '',
          false: '',
        },
        disabled: {
          true: 'cursor-not-allowed opacity-50',
          false: '',
        },
        loading: {
          true: 'relative',
          false: '',
        },
        fullWidth: {
          true: 'w-full',
          false: '',
        },
        iconOnly: {
          true: '',
          false: '',
        },
      },
      compoundVariants: [
        {
          variant: ['secondary', 'ghost'],
          active: true,
          class: 'border-accent-outline bg-accent-soft text-accent-strong',
        },
        { variant: 'primary', active: true, class: 'bg-accent-strong' },
        { variant: 'danger', active: true, class: 'brightness-110' },

        { variant: 'primary', disabled: false, class: 'hover:bg-accent-strong' },
        { variant: 'danger', disabled: false, class: 'hover:brightness-110' },
        {
          variant: 'secondary',
          disabled: false,
          class: 'hover:border-border-strong hover:bg-bg-control-hover',
        },
        { variant: 'ghost', disabled: false, class: 'hover:bg-bg-soft' },

        { variant: 'primary', disabled: false, class: 'active:bg-accent-pressed' },
        { variant: 'danger', disabled: false, class: 'active:brightness-90' },
        {
          variant: 'secondary',
          disabled: false,
          class: 'active:translate-y-px active:bg-bg-control-pressed',
        },
        {
          variant: 'ghost',
          disabled: false,
          class: 'active:translate-y-px active:bg-bg-control-pressed',
        },

        { iconOnly: true, size: 'md', class: 'size-8 min-h-8 p-0' },
        { iconOnly: true, size: 'sm', class: 'size-8 min-h-8 p-0' },
      ],
      defaultVariants: {
        variant: 'secondary',
        size: 'md',
        active: false,
        disabled: false,
        loading: false,
        fullWidth: false,
        iconOnly: false,
      },
    },
  );

  type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger';
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

  const {
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
    cn(
      buttonVariants({ variant, size, active, disabled: isDisabled, loading, fullWidth, iconOnly }),
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
  <span class={cn('inline-flex items-center justify-center gap-1.5', loading && 'opacity-0')}>
    {#if iconOnly}
      <span class="inline-flex size-4 shrink-0">
        {@render children?.()}
      </span>
    {:else}
      {@render children?.()}
    {/if}
  </span>

  {#if loading}
    <span class="pointer-events-none absolute inset-0 grid place-items-center" aria-hidden="true">
      <span
        class="size-3 animate-spin rounded-full border-[1.5px] border-current border-t-transparent motion-reduce:animate-none"
      ></span>
    </span>
  {/if}
</button>
