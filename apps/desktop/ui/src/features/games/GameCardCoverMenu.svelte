<script lang="ts">
  import { onMount, tick } from 'svelte';

  import { isDesktopPreviewMode } from '@shared/api/desktop';
  import { portal } from '@shared/utils/portal';

  import { attachCoverMenuResizeScroll, layoutCoverMenuPanel } from './cover-menu-layout';

  export let title: string;
  export let disabled = false;
  /** When true, disables only “Fetch cover online” (e.g. background auto-fetch already running). */
  export let autoFetchInProgress = false;
  export let hasCover = false;
  export let open = false;

  /** Parent coordinates exclusive menu: pass `true` when this card should show the panel. */
  export let onOpenChange: (next: boolean) => void = (): void => {
    // Intentionally empty.
  };

  export let onFetchCover: () => void | Promise<void> = (): void => {
    // Intentionally empty.
  };

  export let onPickCover: () => void | Promise<void> = (): void => {
    // Intentionally empty.
  };

  export let onClearCover: () => void | Promise<void> = (): void => {
    // Intentionally empty.
  };

  let mounted = false;

  let triggerEl: HTMLButtonElement | null = null;
  let panelEl: HTMLDivElement | null = null;

  let cleanupOpenState: (() => void) | null = null;

  let layoutRafId: number | null = null;
  let focusRafId: number | null = null;

  /**
   * Invalidates async work scheduled for a previous open-state.
   * This prevents late tick()/RAF callbacks from touching a closed or destroyed menu.
   */
  let openStateVersion = 0;

  $: fetchCoverDisabled = disabled || autoFetchInProgress;
  $: pickCoverDisabled = disabled || isDesktopPreviewMode();
  $: clearCoverDisabled = disabled || !hasCover;

  $: syncOpenState(open, mounted);

  $: if (disabled && open) {
    closeMenu({ restoreFocus: false });
  }

  function syncOpenState(isOpen: boolean, isMounted: boolean): void {
    if (!isMounted || !isOpen) {
      teardownOpenState();
      return;
    }

    setupOpenState();
  }

  function setupOpenState(): void {
    if (cleanupOpenState !== null) {
      return;
    }

    document.addEventListener('pointerdown', handleDocumentPointerDown, true);
    document.addEventListener('keydown', handleDocumentKeydown, true);

    const detachLayoutListeners = attachCoverMenuResizeScroll(requestLayout);

    cleanupOpenState = (): void => {
      document.removeEventListener('pointerdown', handleDocumentPointerDown, true);
      document.removeEventListener('keydown', handleDocumentKeydown, true);
      detachLayoutListeners();
    };

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

    if (layoutRafId !== null) {
      cancelAnimationFrame(layoutRafId);
      layoutRafId = null;
    }
  }

  function scheduleInitialOpenWork(): void {
    const version = ++openStateVersion;

    void tick().then(() => {
      if (!isCurrentOpenState(version)) {
        return;
      }

      layoutNow();
      focusFirstMenuItem();

      layoutRafId = requestAnimationFrame(() => {
        layoutRafId = null;

        if (isCurrentOpenState(version)) {
          layoutNow();
        }
      });
    });
  }

  function isCurrentOpenState(version: number): boolean {
    return mounted && open && cleanupOpenState !== null && openStateVersion === version;
  }

  function requestLayout(): void {
    if (layoutRafId !== null) {
      return;
    }

    layoutRafId = requestAnimationFrame(() => {
      layoutRafId = null;
      layoutNow();
    });
  }

  function layoutNow(): void {
    if (!open || triggerEl === null || panelEl === null) {
      return;
    }

    layoutCoverMenuPanel(triggerEl, panelEl);
  }

  function handleDocumentPointerDown(event: PointerEvent): void {
    if (!open) {
      return;
    }

    if (eventTargetsNode(event, triggerEl) || eventTargetsNode(event, panelEl)) {
      return;
    }

    closeMenu({ restoreFocus: false });
  }

  function handleDocumentKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Escape' || !open) {
      return;
    }

    event.preventDefault();
    closeMenu({ restoreFocus: true });
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
        focusMenuItemAt(getEnabledMenuItems().length - 1);
        break;
    }
  }

  function eventTargetsNode(event: Event, node: Node | null): boolean {
    if (node === null) {
      return false;
    }

    if (event.composedPath().includes(node)) {
      return true;
    }

    return event.target instanceof Node && node.contains(event.target);
  }

  function getEnabledMenuItems(): HTMLButtonElement[] {
    if (panelEl === null) {
      return [];
    }

    return Array.from(
      panelEl.querySelectorAll<HTMLButtonElement>('.card-menu-item:not(:disabled)'),
    );
  }

  function focusFirstMenuItem(): void {
    const enabledItems = getEnabledMenuItems();

    if (enabledItems.length > 0) {
      enabledItems[0].focus({ preventScroll: true });
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
    focusMenuItemAt(activeIndex + 1);
  }

  function focusPreviousMenuItem(): void {
    const enabledItems = getEnabledMenuItems();

    if (enabledItems.length === 0) {
      return;
    }

    const activeIndex = enabledItems.indexOf(document.activeElement as HTMLButtonElement);
    focusMenuItemAt(activeIndex <= 0 ? enabledItems.length - 1 : activeIndex - 1);
  }

  function focusMenuItemAt(index: number): void {
    const enabledItems = getEnabledMenuItems();

    if (enabledItems.length === 0) {
      return;
    }

    const normalizedIndex =
      ((index % enabledItems.length) + enabledItems.length) % enabledItems.length;

    enabledItems[normalizedIndex].focus({ preventScroll: true });
  }

  export function focusTrigger(): void {
    if (focusRafId !== null) {
      cancelAnimationFrame(focusRafId);
    }

    focusRafId = requestAnimationFrame(() => {
      focusRafId = null;
      triggerEl?.focus({ preventScroll: true });
    });
  }

  function openMenu(): void {
    if (disabled || open) {
      return;
    }

    onOpenChange(true);
  }

  function closeMenu(options: { restoreFocus?: boolean } = {}): void {
    if (!open) {
      return;
    }

    onOpenChange(false);

    if (options.restoreFocus === true) {
      focusTrigger();
    }
  }

  function toggleMenu(): void {
    if (open) {
      closeMenu({ restoreFocus: false });
      return;
    }

    openMenu();
  }

  function handleTriggerClick(event: MouseEvent): void {
    event.stopPropagation();
    toggleMenu();
  }

  function handleMenuActionClick(event: MouseEvent, action: () => void | Promise<void>): void {
    event.stopPropagation();

    closeMenu({ restoreFocus: false });

    try {
      void Promise.resolve(action()).catch(reportMenuActionError);
    } catch (error) {
      reportMenuActionError(error);
    }
  }

  function reportMenuActionError(error: unknown): void {
    console.error('Cover menu action failed.', error);
  }

  onMount(() => {
    mounted = true;

    return () => {
      mounted = false;

      teardownOpenState();

      if (focusRafId !== null) {
        cancelAnimationFrame(focusRafId);
        focusRafId = null;
      }
    };
  });
</script>

<div class="card-corner-menu">
  <button
    bind:this={triggerEl}
    type="button"
    class="card-menu-trigger"
    aria-label={`Cover options for ${title}`}
    aria-haspopup="menu"
    aria-expanded={open ? 'true' : 'false'}
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

  {#if open}
    <div
      bind:this={panelEl}
      class="card-menu-panel"
      role="menu"
      tabindex="-1"
      use:portal
      onkeydown={handlePanelKeydown}
    >
      <button
        type="button"
        class="card-menu-item"
        role="menuitem"
        tabindex="-1"
        disabled={fetchCoverDisabled}
        title="Uses Steam or GOG CDN when possible; otherwise SteamGridDB if you configured an API key in Settings."
        onclick={(event: MouseEvent): void => {
          handleMenuActionClick(event, onFetchCover);
        }}
      >
        Fetch cover online
      </button>

      <button
        type="button"
        class="card-menu-item"
        role="menuitem"
        tabindex="-1"
        disabled={pickCoverDisabled}
        title="Choose a PNG, JPG, WebP, or GIF from disk and save it as this game’s cover."
        onclick={(event: MouseEvent): void => {
          handleMenuActionClick(event, onPickCover);
        }}
      >
        Use image file as cover…
      </button>

      <button
        type="button"
        class="card-menu-item card-menu-item-danger"
        role="menuitem"
        tabindex="-1"
        disabled={clearCoverDisabled}
        title="Removes the saved cover file and clears the thumbnail for this game."
        onclick={(event: MouseEvent): void => {
          handleMenuActionClick(event, onClearCover);
        }}
      >
        Remove saved cover
      </button>
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
