/** Coalesces tab-entry and focus refreshes for the currently visible NVIDIA view. */
export function createNvapiViewRefresh(refresh: (viewKey: string) => void) {
  let initialized = false;
  let currentView: string | null = null;
  let currentOwner: string | null = null;
  let refreshRequested = false;
  let ready = false;

  function flush(): void {
    if (
      !refreshRequested ||
      currentView === null ||
      !ready ||
      document.visibilityState !== 'visible'
    ) {
      return;
    }
    refreshRequested = false;
    refresh(currentView);
  }

  function sync(viewKey: string | null, isReady: boolean, ownerKey: string | null = null): void {
    if (!initialized || ownerKey !== currentOwner || viewKey === null) {
      refreshRequested = false;
    } else if (viewKey !== currentView) {
      refreshRequested = true;
    }
    initialized = true;
    currentOwner = ownerKey;
    currentView = viewKey;
    ready = isReady;
    flush();
  }

  function onFocus(): void {
    if (currentView !== null && document.visibilityState === 'visible') {
      refreshRequested = true;
      flush();
    }
  }

  window.addEventListener('focus', onFocus);

  return {
    sync,
    destroy(): void {
      window.removeEventListener('focus', onFocus);
      currentView = null;
      currentOwner = null;
      refreshRequested = false;
    },
  };
}
