<script lang="ts">
  import { cn, isHTMLElement, isNode, normalizeA11yTextProps } from '@shared/utils';
  import { onDestroy, tick } from 'svelte';
  import type { Snippet } from 'svelte';

  import { registerDismissableLayer, type DismissableLayerEvent } from './dismissable-layer';
  import type { PopoverOpenChangeEvent, PopoverOpenChangeReason } from './popover-types';

  type PopoverRole = 'dialog' | 'menu' | 'listbox' | 'region';
  type PopoverAlign = 'start' | 'end';

  type FocusTarget = HTMLElement | null;
  type FocusTargetResolver = FocusTarget | (() => FocusTarget);

  type ScheduledDomTask = {
    cancel: () => void;
  };

  type Props = {
    open?: boolean;
    closeOnEscape?: boolean;
    closeOnOutsidePointerDown?: boolean;
    renderPanel?: boolean;
    className?: string;
    panelClassName?: string;
    align?: PopoverAlign;
    sideOffset?: string;
    role?: PopoverRole;

    referenceElement?: HTMLElement | null;
    contentElement?: HTMLElement | null;
    insideElements?: readonly (HTMLElement | null | undefined)[];

    initialFocus?: FocusTargetResolver;
    restoreFocusTarget?: FocusTargetResolver;
    restoreFocusOnClose?: boolean;

    'aria-label'?: string | null;
    'aria-labelledby'?: string | null;
    'aria-describedby'?: string | null;

    onOpenChange?: (event: PopoverOpenChangeEvent) => void;

    /** Fallback for older consumers. Prefer `referenceElement`. */
    anchor?: HTMLElement | null;

    children?: Snippet;
  };

  const {
    open = false,
    closeOnEscape = true,
    closeOnOutsidePointerDown = true,
    renderPanel = true,
    className = '',
    panelClassName = '',
    align = 'end',
    sideOffset = '0.5rem',
    role = 'dialog',

    referenceElement = null,
    contentElement = null,
    insideElements = [],

    initialFocus = null,
    restoreFocusTarget = null,
    restoreFocusOnClose = true,

    'aria-label': ariaLabel,
    'aria-labelledby': ariaLabelledBy,
    'aria-describedby': ariaDescribedBy,

    onOpenChange,
    anchor = null,
    children,
  }: Props = $props();

  let rootElement = $state<HTMLDivElement | null>(null);
  let panelElement = $state<HTMLDivElement | null>(null);

  let disposeDismissableLayer: (() => void) | null = null;

  let initialFocusTask: ScheduledDomTask | null = null;
  let restoreFocusTask: ScheduledDomTask | null = null;

  let initialFocusToken = 0;
  let restoreFocusToken = 0;

  let destroyed = false;
  let hasSyncedLifecycle = false;
  let previousOpen = false;
  let previousPanelRenderable = false;

  const rootClass = $derived(cn('relative inline-flex', className));
  const panelClass = $derived(
    cn(
      'absolute top-[calc(100%+var(--popover-side-offset))] z-20 grid gap-2',
      'max-w-none',
      'border border-border-strong/90',
      'bg-bg-panel',
      'shadow-2xl',
      'rounded-2xl',
      align === 'start' ? 'right-auto left-0' : 'right-0 left-auto',
      panelClassName,
    ),
  );
  const isPanelRenderable = $derived(open && renderPanel);

  const fallbackReferenceElement = $derived(referenceElement ?? anchor);

  const a11yText = $derived(
    normalizeA11yTextProps({
      label: ariaLabel,
      labelledBy: ariaLabelledBy,
      describedBy: ariaDescribedBy,
    }),
  );

  $effect(() => {
    syncPopoverLifecycle(open, isPanelRenderable);
  });

  function syncPopoverLifecycle(isOpen: boolean, panelRenderable: boolean): void {
    const wasOpen = hasSyncedLifecycle ? previousOpen : false;
    const wasPanelRenderable = hasSyncedLifecycle ? previousPanelRenderable : false;

    const didClose = hasSyncedLifecycle && wasOpen && !isOpen;
    const didBecomePanelRenderable = panelRenderable && !wasPanelRenderable;

    previousOpen = isOpen;
    previousPanelRenderable = panelRenderable;
    hasSyncedLifecycle = true;

    if (!isOpen) {
      deactivateDismissableLayer();

      if (didClose && restoreFocusOnClose) {
        scheduleRestoreFocus();
      }

      return;
    }

    cancelScheduledRestoreFocus();
    activateDismissableLayer();

    if (didBecomePanelRenderable) {
      scheduleInitialFocus();
      return;
    }

    if (!panelRenderable) {
      cancelScheduledInitialFocus();
    }
  }

  function requestOpenChange(
    nextOpen: boolean,
    reason: PopoverOpenChangeReason = 'programmatic',
    originalEvent?: PointerEvent | KeyboardEvent,
  ): void {
    if (nextOpen === open) {
      return;
    }

    onOpenChange?.({
      open: nextOpen,
      reason,
      originalEvent,
    });
  }

  function activateDismissableLayer(): void {
    if (disposeDismissableLayer !== null || !canUseDom()) {
      return;
    }

    disposeDismissableLayer = registerDismissableLayer({
      isEnabled: () => open,
      isEventInside,
      onDismiss: handleDismiss,
    });
  }

  function deactivateDismissableLayer(): void {
    disposeDismissableLayer?.();
    disposeDismissableLayer = null;

    cancelScheduledInitialFocus();
  }

  function handleDismiss(event: DismissableLayerEvent): void {
    if (!shouldCloseOnDismiss(event.reason)) {
      return;
    }

    requestOpenChange(false, event.reason, event.originalEvent);
  }

  function shouldCloseOnDismiss(reason: DismissableLayerEvent['reason']): boolean {
    if (reason === 'escape') {
      return closeOnEscape;
    }

    return closeOnOutsidePointerDown;
  }

  function isEventInside(event: Event): boolean {
    const target = event.target;

    if (!isNode(target)) {
      return false;
    }

    const eventPath = getEventPath(event);

    return getInsideElements().some((element) => {
      return eventPath.includes(element) || element.contains(target);
    });
  }

  function getInsideElements(): HTMLElement[] {
    return uniqueElements([
      referenceElement,
      anchor,
      rootElement,
      contentElement,
      panelElement,
      ...insideElements,
    ]);
  }

  function uniqueElements(values: readonly unknown[]): HTMLElement[] {
    const elements: HTMLElement[] = [];
    const seenElements: HTMLElement[] = [];

    for (const value of values) {
      if (!isHTMLElement(value) || seenElements.some((e) => e === value)) {
        continue;
      }

      seenElements.push(value);
      elements.push(value);
    }

    return elements;
  }

  function getEventPath(event: Event): readonly EventTarget[] {
    if (typeof event.composedPath === 'function') {
      return event.composedPath();
    }

    return event.target ? [event.target] : [];
  }

  function scheduleInitialFocus(): void {
    if (initialFocusTask !== null) {
      return;
    }

    const currentToken = ++initialFocusToken;

    initialFocusTask = scheduleDomTask(() => {
      initialFocusTask = null;
      void focusInitialTarget(currentToken);
    });
  }

  function cancelScheduledInitialFocus(): void {
    initialFocusToken += 1;
    initialFocusTask = cancelDomTask(initialFocusTask);
  }

  async function focusInitialTarget(token: number): Promise<void> {
    await tick();

    if (token !== initialFocusToken || destroyed || !open || !renderPanel) {
      return;
    }

    focusElement(resolveFocusTarget(initialFocus) ?? panelElement);
  }

  function scheduleRestoreFocus(): void {
    if (restoreFocusTask !== null) {
      return;
    }

    const currentToken = ++restoreFocusToken;

    restoreFocusTask = scheduleDomTask(() => {
      restoreFocusTask = null;
      void focusRestoreTarget(currentToken);
    });
  }

  function cancelScheduledRestoreFocus(): void {
    restoreFocusToken += 1;
    restoreFocusTask = cancelDomTask(restoreFocusTask);
  }

  async function focusRestoreTarget(token: number): Promise<void> {
    await tick();

    if (token !== restoreFocusToken || destroyed || open) {
      return;
    }

    focusElement(resolveFocusTarget(restoreFocusTarget) ?? fallbackReferenceElement ?? rootElement);
  }

  function resolveFocusTarget(value: FocusTargetResolver): FocusTarget {
    if (typeof value !== 'function') {
      return value;
    }

    try {
      return value();
    } catch {
      return null;
    }
  }

  function focusElement(element: FocusTarget): void {
    if (!element?.isConnected) {
      return;
    }

    try {
      element.focus({ preventScroll: true });
    } catch {
      element.focus();
    }
  }

  function scheduleDomTask(callback: () => void): ScheduledDomTask | null {
    if (!canUseDom()) {
      return null;
    }

    if (
      typeof window.requestAnimationFrame === 'function' &&
      typeof window.cancelAnimationFrame === 'function'
    ) {
      const frameId = window.requestAnimationFrame(callback);

      return {
        cancel: () => {
          window.cancelAnimationFrame(frameId);
        },
      };
    }

    const timeoutId = window.setTimeout(callback, 0);

    return {
      cancel: () => {
        window.clearTimeout(timeoutId);
      },
    };
  }

  function cancelDomTask(task: ScheduledDomTask | null): null {
    task?.cancel();

    return null;
  }

  function canUseDom(): boolean {
    return typeof window !== 'undefined' && typeof document !== 'undefined';
  }

  onDestroy(() => {
    destroyed = true;

    deactivateDismissableLayer();
    cancelScheduledRestoreFocus();
  });
</script>

<div class={rootClass} bind:this={rootElement}>
  {#if open && renderPanel}
    <div
      bind:this={panelElement}
      class={panelClass}
      {role}
      tabindex="-1"
      aria-label={a11yText.ariaLabel}
      aria-labelledby={a11yText.ariaLabelledBy}
      aria-describedby={a11yText.ariaDescribedBy}
      style:--popover-side-offset={sideOffset}
    >
      {@render children?.()}
    </div>
  {/if}
</div>
