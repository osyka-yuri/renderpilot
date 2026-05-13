<script lang="ts">
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import InfoIcon from '@lucide/svelte/icons/info';
  import Loader2Icon from '@lucide/svelte/icons/loader-2';
  import OctagonXIcon from '@lucide/svelte/icons/octagon-x';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import { mode } from 'mode-watcher';
  import { Toaster as Sonner, type ToasterProps as SonnerProps } from 'svelte-sonner';

  const TOASTER_CLASS = 'toaster group';

  const TOASTER_STYLE = [
    '--normal-bg: var(--color-popover)',
    '--normal-text: var(--color-popover-foreground)',
    '--normal-border: var(--color-border)',
  ].join('; ');

  const ICON_CLASS = 'size-4';
  const LOADING_ICON_CLASS = `${ICON_CLASS} animate-spin`;

  let { class: className, style, theme, ...restProps }: SonnerProps = $props();

  const resolvedTheme = $derived(theme ?? mode.current);
  const toasterClass = $derived(mergeClassName(TOASTER_CLASS, className));
  const toasterStyle = $derived(mergeCssText(TOASTER_STYLE, style));

  function normalizeString(value: unknown): string {
    return typeof value === 'string' ? value.trim() : '';
  }

  function normalizeCssText(value: unknown): string {
    return normalizeString(value).replace(/;+$/, '');
  }

  function mergeClassName(baseClassName: string, className: unknown): string {
    const normalizedClassName = normalizeString(className);

    return normalizedClassName ? `${baseClassName} ${normalizedClassName}` : baseClassName;
  }

  function mergeCssText(baseCssText: string, cssText: unknown): string {
    const normalizedCssText = normalizeCssText(cssText);

    return normalizedCssText ? `${baseCssText}; ${normalizedCssText}` : baseCssText;
  }
</script>

<Sonner {...restProps} theme={resolvedTheme} class={toasterClass} style={toasterStyle}>
  {#snippet loadingIcon()}
    <Loader2Icon class={LOADING_ICON_CLASS} aria-hidden="true" />
  {/snippet}

  {#snippet successIcon()}
    <CircleCheckIcon class={ICON_CLASS} aria-hidden="true" />
  {/snippet}

  {#snippet errorIcon()}
    <OctagonXIcon class={ICON_CLASS} aria-hidden="true" />
  {/snippet}

  {#snippet infoIcon()}
    <InfoIcon class={ICON_CLASS} aria-hidden="true" />
  {/snippet}

  {#snippet warningIcon()}
    <TriangleAlertIcon class={ICON_CLASS} aria-hidden="true" />
  {/snippet}
</Sonner>
