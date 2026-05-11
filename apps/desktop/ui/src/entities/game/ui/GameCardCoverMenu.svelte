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
  import { portal, attachFloatingPanelResizeScroll, layoutFloatingPanel } from '@shared/utils';

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

  let {
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

<div class="card-corner-menu">
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
    class="card-menu-trigger"
    aria-label={`Cover options for ${title}`}
    aria-haspopup="menu"
    aria-expanded={isMenuOpen ? 'true' : 'false'}
    aria-controls={isMenuOpen ? panelId : undefined}
    {disabled}
    onclick={handleTriggerClick}
  >
    <svg
      class="card-menu-trigger-icon"
      viewBox="0 0 16 16"
      width="16"
      height="16"
      aria-hidden="true"
    >
      <circle cx="3" cy="8" r="1.5" fill="currentColor" />
      <circle cx="8" cy="8" r="1.5" fill="currentColor" />
      <circle cx="13" cy="8" r="1.5" fill="currentColor" />
    </svg>
  </button>

  {#if isMenuOpen}
    <div
      bind:this={panelEl}
      id={panelId}
      class="card-menu-panel"
      role="menu"
      tabindex="-1"
      use:portal
      onkeydown={handlePanelKeydown}
    >
      {#each menuActions as action (action.id)}
        <button
          type="button"
          class="card-menu-item"
          class:card-menu-item-danger={action.danger === true}
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

<style>
  .card-corner-menu {
    position: absolute;
    top: 0;
    right: 0;
    z-index: 4;

    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.25rem;
    pointer-events: auto;
  }

  .card-menu-trigger {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.75rem;
    height: 1.75rem;
    padding: 0;
    border: 1px solid color-mix(in srgb, var(--border-subtle) 80%, transparent);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--bg-card) 82%, transparent);
    color: var(--text-strong);
    line-height: 0;
    cursor: pointer;
    backdrop-filter: blur(4px);
  }

  .card-menu-trigger-icon {
    display: block;
    flex-shrink: 0;
  }

  .card-menu-trigger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--bg-card) 92%, transparent);
  }

  .card-menu-trigger:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .card-menu-trigger:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .card-menu-panel {
    min-width: 15rem;
    max-width: min(18rem, calc(100vw - 2rem));
    padding: 0.35rem;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--border-subtle) 88%, transparent);
    background: var(--bg-card);
    box-shadow:
      0 8px 28px color-mix(in srgb, black 28%, transparent),
      0 1px 0 color-mix(in srgb, white 8%, transparent);
  }

  .card-menu-item {
    display: block;
    width: 100%;
    padding: 0.45rem 0.55rem;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-strong);
    font-size: 0.8125rem;
    text-align: left;
    cursor: pointer;
  }

  .card-menu-item:hover:not(:disabled) {
    background: var(--accent-soft);
  }

  .card-menu-item:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .card-menu-item:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .card-menu-item-danger:not(:disabled) {
    color: color-mix(in srgb, var(--accent-strong) 70%, var(--text-strong));
  }

  @media (max-width: 720px) {
    .card-corner-menu {
      top: calc(min(7.125rem, 40vw) * 900 / 600 + var(--space-3));
    }
  }
</style>
