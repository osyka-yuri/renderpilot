<script lang="ts">
  import { cva } from 'class-variance-authority';
  import { cn, normalizeA11yTextProps } from '@shared/utils';
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  const switchRootVariants = cva(
    'group flex w-full min-w-0 touch-manipulation items-center justify-between gap-4 border-0 bg-transparent p-0 text-start select-none focus-visible:outline-none',
    {
      variants: {
        disabled: {
          true: 'cursor-not-allowed opacity-50',
          false: '',
        },
        stackOnMobile: {
          true: 'max-md:flex-col max-md:items-stretch max-md:gap-3',
          false: '',
        },
      },
      defaultVariants: {
        disabled: false,
        stackOnMobile: true,
      },
    },
  );

  type SwitchState = 'checked' | 'unchecked';

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
    | 'onclick'
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

    onclick?: ((event: MouseEvent) => void) | undefined;
    onCheckedChange?: ((nextChecked: boolean, event: MouseEvent) => void) | undefined;
  };

  const {
    children,

    checked = false,
    disabled = false,
    stackOnMobile = true,

    class: className = '',
    title = null,

    'aria-label': ariaLabel = null,
    'aria-labelledby': ariaLabelledBy = null,
    'aria-describedby': ariaDescribedBy = null,

    onclick: onClick,
    onCheckedChange,

    ...restProps
  }: SwitchProps = $props();

  const switchState = $derived<SwitchState>(checked ? 'checked' : 'unchecked');
  const ariaChecked = $derived(checked ? 'true' : 'false');

  const a11yText = $derived(
    normalizeA11yTextProps({
      label: ariaLabel,
      labelledBy: ariaLabelledBy,
      describedBy: ariaDescribedBy,
      title,
    }),
  );

  const switchClass = $derived(cn(switchRootVariants({ disabled, stackOnMobile }), className));
  const stackOnMobileAttribute = $derived(stackOnMobile ? 'true' : undefined);

  const trackClass = $derived(
    cn(
      'relative h-5 w-10 shrink-0 grow-0 basis-10 rounded-full border',
      'transition duration-140 motion-reduce:transition-none',
      checked ? 'border-transparent bg-accent' : 'border-border-control bg-bg-control',
      !disabled && !checked && 'group-hover:bg-bg-control-hover',
      !disabled && checked && 'group-hover:bg-accent-strong',
      'group-focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-bg-base',
    ),
  );

  const thumbClass = $derived(
    cn(
      'pointer-events-none absolute top-1/2 size-3.5 -translate-y-1/2 rounded-full',
      'transition-[inset-inline-start,background-color] duration-140 motion-reduce:transition-none',
      checked
        ? 'inset-s-[calc(100%-0.125rem-0.875rem)] bg-accent-contrast'
        : 'inset-s-0.5 bg-text-soft',
    ),
  );

  function handleClick(event: MouseEvent): void {
    onClick?.(event);

    if (event.defaultPrevented || disabled) {
      return;
    }

    onCheckedChange?.(!checked, event);
  }
</script>

<button
  {...restProps}
  type="button"
  role="switch"
  aria-checked={ariaChecked}
  aria-label={a11yText.ariaLabel}
  aria-labelledby={a11yText.ariaLabelledBy}
  aria-describedby={a11yText.ariaDescribedBy}
  title={a11yText.title}
  {disabled}
  class={switchClass}
  data-state={switchState}
  data-stack-on-mobile={stackOnMobileAttribute}
  onclick={handleClick}
>
  <span class="min-w-0 flex-auto">
    {@render children?.()}
  </span>

  <span class={trackClass} aria-hidden="true">
    <span class={thumbClass}></span>
  </span>
</button>
