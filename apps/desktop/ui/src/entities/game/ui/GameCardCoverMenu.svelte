<script module lang="ts">
  let nextCoverMenuPanelId = 0;

  function createCoverMenuPanelId(): string {
    nextCoverMenuPanelId += 1;

    return `cover-menu-${nextCoverMenuPanelId}`;
  }
</script>

<script lang="ts">
  import { onMount, tick } from 'svelte';

  import { Popover, type PopoverOpenChangeEvent } from '@shared/ui';
  import { cn, portal, attachFloatingPanelResizeScroll, layoutFloatingPanel } from '@shared/utils';

  type MenuActionId = 'fetch-cover' | 'pick-cover' | 'clear-cover';

  type MenuActionHandler = () => void | Promise<void>;

  type MenuAction = {
    id: MenuActionId;
    label: string;
    title: string;
    disabled: boolean;
    danger?: boolean;
    handler: MenuActionHandler;
  };

  type Props = {
    title: string;
    disabled?: boolean;
    pickDisabled?: boolean;
    autoFetchInProgress?: boolean;
    hasCover?: boolean;
    open?: boolean;

    onOpenChange?: (next: boolean) => void;
    onFetchCover?: MenuActionHandler;
    onPickCover?: MenuActionHandler;
    onClearCover?: MenuActionHandler;
  };

  const ENABLED_MENU_ITEM_SELECTOR = '.card-menu-item:not(:disabled)';
  const noopMenuAction: MenuActionHandler = () => undefined;

  const {
    title,
    disabled = false,
    pickDisabled = false,
    autoFetchInProgress = false,
    hasCover = false,
    open = false,

    onOpenChange,
    onFetchCover,
    onPickCover,
    onClearCover,
  }: Props = $props();

  let mounted = $state(false);

  let triggerEl = $state<HTMLButtonElement | null>(null);
  let panelEl = $state<HTMLDivElement | null>(null);

  const panelId = createCoverMenuPanelId();

  let cleanupOpenState: (() => void) | null = null;

  let layoutRafId: number | null = null;
  let focusRafId: number | null = null;

  /**
   * Invalidates async work scheduled for a previous open-state.
   * Prevents late tick()/RAF callbacks from touching a closed or destroyed menu.
   */
  let openStateVersion = 0;

  const isMenuOpen = $derived(open && !disabled);

  const menuActions = $derived<MenuAction[]>([
    {
      id: 'fetch-cover',
      label: 'Fetch cover online',
      disabled: disabled || autoFetchInProgress,
      title:
        'Uses Steam or GOG CDN when possible; otherwise SteamGridDB if you configured an API key in Settings.',
      handler: onFetchCover ?? noopMenuAction,
    },
    {
      id: 'pick-cover',
      label: 'Use image file as cover…',
      disabled: disabled || pickDisabled,
      title: 'Choose a PNG, JPG, WebP, or GIF from disk and save it as this game’s cover.',
      handler: onPickCover ?? noopMenuAction,
    },
    {
      id: 'clear-cover',
      label: 'Remove saved cover',
      disabled: disabled || !hasCover,
      danger: true,
      title: 'Removes the saved cover file and clears the thumbnail for this game.',
      handler: onClearCover ?? noopMenuAction,
    },
  ]);

  $effect(() => {
    syncOpenLifecycle(isMenuOpen, mounted);
  });

  $effect(() => {
    if (disabled && open) {
      closeMenu({ restoreFocus: false });
    }
  });

  function syncOpenLifecycle(shouldBeOpen: boolean, shouldBeMounted: boolean): void {
    if (!shouldBeMounted || !shouldBeOpen) {
      teardownOpenState();
      return;
    }

    setupOpenState();
  }

  function setupOpenState(): void {
    if (cleanupOpenState !== null) {
      return;
    }

    cleanupOpenState = attachFloatingPanelResizeScroll(requestLayout);
    scheduleInitialOpenWork();
  }

  function teardownOpenState(): void {
    const cleanup = cleanupOpenState;

    cleanupOpenState = null;
    cleanup?.();

    invalidateScheduledOpenWork();
  }

  function invalidateScheduledOpenWork(): void {
    openStateVersion += 1;
    cancelLayoutFrame();
  }

  function scheduleInitialOpenWork(): void {
    const version = ++openStateVersion;

    void tick().then(() => {
      if (!isCurrentOpenState(version)) {
        return;
      }

      layoutNow();
      focusFirstMenuItem();
      scheduleLayoutFrame(version);
    });
  }

  function isCurrentOpenState(version: number): boolean {
    return mounted && isMenuOpen && cleanupOpenState !== null && openStateVersion === version;
  }

  function requestLayout(): void {
    scheduleLayoutFrame();
  }

  function scheduleLayoutFrame(version?: number): void {
    if (!mounted || !isMenuOpen || layoutRafId !== null) {
      return;
    }

    layoutRafId = requestAnimationFrame(() => {
      layoutRafId = null;

      if (version !== undefined && !isCurrentOpenState(version)) {
        return;
      }

      layoutNow();
    });
  }

  function cancelLayoutFrame(): void {
    if (layoutRafId === null) {
      return;
    }

    cancelAnimationFrame(layoutRafId);
    layoutRafId = null;
  }

  function cancelFocusFrame(): void {
    if (focusRafId === null) {
      return;
    }

    cancelAnimationFrame(focusRafId);
    focusRafId = null;
  }

  function layoutNow(): void {
    if (!mounted || !isMenuOpen || triggerEl === null || panelEl === null) {
      return;
    }

    layoutFloatingPanel(triggerEl, panelEl);
  }

  function handlePanelKeydown(event: KeyboardEvent): void {
    switch (event.key) {
      case 'ArrowDown':
      case 'ArrowRight':
        event.preventDefault();
        focusNextMenuItem();
        break;

      case 'ArrowUp':
      case 'ArrowLeft':
        event.preventDefault();
        focusPreviousMenuItem();
        break;

      case 'Home':
        event.preventDefault();
        focusMenuItemAt(0);
        break;

      case 'End':
        event.preventDefault();
        focusMenuItemAt(-1);
        break;

      case 'Tab':
        closeMenu({ restoreFocus: false });
        break;
    }
  }

  function getEnabledMenuItems(): HTMLButtonElement[] {
    if (panelEl === null) {
      return [];
    }

    return Array.from(panelEl.querySelectorAll<HTMLButtonElement>(ENABLED_MENU_ITEM_SELECTOR));
  }

  function focusFirstMenuItem(): void {
    const enabledItems = getEnabledMenuItems();

    if (enabledItems.length > 0) {
      enabledItems[0]?.focus({ preventScroll: true });
      return;
    }

    panelEl?.focus({ preventScroll: true });
  }

  function focusNextMenuItem(): void {
    const enabledItems = getEnabledMenuItems();

    if (enabledItems.length === 0) {
      return;
    }

    const activeIndex = enabledItems.indexOf(document.activeElement as HTMLButtonElement);

    focusMenuItemAt(activeIndex + 1, enabledItems);
  }

  function focusPreviousMenuItem(): void {
    const enabledItems = getEnabledMenuItems();

    if (enabledItems.length === 0) {
      return;
    }

    const activeIndex = enabledItems.indexOf(document.activeElement as HTMLButtonElement);
    const previousIndex = activeIndex === -1 ? enabledItems.length - 1 : activeIndex - 1;

    focusMenuItemAt(previousIndex, enabledItems);
  }

  function focusMenuItemAt(index: number, enabledItems = getEnabledMenuItems()): void {
    if (enabledItems.length === 0) {
      return;
    }

    const normalizedIndex =
      ((index % enabledItems.length) + enabledItems.length) % enabledItems.length;

    enabledItems[normalizedIndex]?.focus({ preventScroll: true });
  }

  export function focusTrigger(): void {
    if (!mounted) {
      return;
    }

    cancelFocusFrame();

    focusRafId = requestAnimationFrame(() => {
      focusRafId = null;

      if (!mounted) {
        return;
      }

      triggerEl?.focus({ preventScroll: true });
    });
  }

  function openMenu(): void {
    if (disabled || open) {
      return;
    }

    onOpenChange?.(true);
  }

  function closeMenu(options: { restoreFocus?: boolean } = {}): void {
    if (!open) {
      return;
    }

    onOpenChange?.(false);

    if (options.restoreFocus === true) {
      focusTrigger();
    }
  }

  function toggleMenu(): void {
    if (isMenuOpen) {
      closeMenu({ restoreFocus: false });
      return;
    }

    openMenu();
  }

  function handleTriggerClick(event: MouseEvent): void {
    event.stopPropagation();
    toggleMenu();
  }

  function handlePopoverOpenChange(event: PopoverOpenChangeEvent): void {
    if (event.open) {
      openMenu();
      return;
    }

    closeMenu({ restoreFocus: false });

    // Popover handles focus restore on dismiss paths.
  }

  function handleMenuActionClick(event: MouseEvent, action: MenuAction): void {
    event.preventDefault();
    event.stopPropagation();

    if (action.disabled) {
      return;
    }

    closeMenu({ restoreFocus: false });

    void runMenuAction(action);
  }

  async function runMenuAction(action: MenuAction): Promise<void> {
    try {
      await action.handler();
    } catch (error) {
      reportMenuActionError(action.id, error);
    }
  }

  function reportMenuActionError(actionId: MenuActionId, error: unknown): void {
    console.error(`Cover menu action "${actionId}" failed.`, error);
  }

  onMount(() => {
    mounted = true;

    return () => {
      mounted = false;

      teardownOpenState();
      cancelLayoutFrame();
      cancelFocusFrame();
    };
  });
</script>

<div
  class="
    pointer-events-auto absolute top-0 right-0 z-4 flex flex-col items-end gap-1
    max-md:top-[calc(min(7.125rem,40vw)*900/600+0.75rem)]
  "
>
  <Popover
    anchor={triggerEl}
    referenceElement={triggerEl}
    contentElement={panelEl}
    open={isMenuOpen}
    renderPanel={false}
    initialFocus={() => panelEl}
    restoreFocusTarget={() => triggerEl}
    restoreFocusOnClose
    closeOnEscape
    closeOnOutsidePointerDown
    onOpenChange={handlePopoverOpenChange}
  />

  <button
    bind:this={triggerEl}
    type="button"
    class={cn(
      'inline-flex size-7 items-center justify-center rounded-2xl border',
      'border-border-subtle/80 bg-bg-card/80 p-0 leading-0 text-text-strong',
      'backdrop-blur-xs',
      'hover:bg-bg-card/90',
      'focus-visible:outline-2 focus-visible:outline-offset-2',
      'focus-visible:outline-accent',
      'disabled:cursor-not-allowed disabled:opacity-45',
    )}
    aria-label={`Cover options for ${title}`}
    aria-haspopup="menu"
    aria-expanded={isMenuOpen ? 'true' : 'false'}
    aria-controls={isMenuOpen ? panelId : undefined}
    {disabled}
    onclick={handleTriggerClick}
  >
    <svg class="block shrink-0" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
      <circle cx="3" cy="8" r="1.5" fill="currentColor" />
      <circle cx="8" cy="8" r="1.5" fill="currentColor" />
      <circle cx="13" cy="8" r="1.5" fill="currentColor" />
    </svg>
  </button>

  {#if isMenuOpen}
    <div
      bind:this={panelEl}
      id={panelId}
      class={cn(
        'max-w-72 min-w-60 rounded-2xl border border-border-subtle/90 bg-bg-card',
        'p-1.5 shadow-xl',
      )}
      role="menu"
      tabindex="-1"
      use:portal
      onkeydown={handlePanelKeydown}
    >
      {#each menuActions as action (action.id)}
        <button
          type="button"
          class={cn(
            'block w-full rounded-2xl border-none bg-transparent p-1.5',
            'text-left text-xs text-text-strong',
            'hover:bg-accent-soft',
            'focus-visible:outline-2 focus-visible:-outline-offset-2',
            'focus-visible:outline-accent',
            'disabled:cursor-not-allowed disabled:opacity-45',
            action.danger === true && !action.disabled && 'text-accent-strong/70',
          )}
          role="menuitem"
          tabindex="-1"
          disabled={action.disabled}
          title={action.title}
          onclick={(event: MouseEvent): void => {
            handleMenuActionClick(event, action);
          }}
        >
          {action.label}
        </button>
      {/each}
    </div>
  {/if}
</div>
