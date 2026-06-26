import { createFileDropTarget } from '@shared/api';

/**
 * Shared webview drag-drop wiring for the add-on file-install panels. Registers the
 * Tauri file-drop listener against the bound element while the component is mounted
 * and reports whether a drag is currently over it. The async setup + teardown
 * lifecycle lives here so both install panels share one copy.
 *
 * Call it once in a component's script with a getter for the drop element (bound via
 * `bind:this`) and the drop handler; read `dragActive` for the hover styling.
 */
export function createAddonDrop(
  getElement: () => HTMLElement | null,
  onDrop: (paths: string[]) => void,
) {
  let dragActive = $state(false);

  $effect(() => {
    let cleanup: (() => void) | null = null;
    let disposed = false;
    void createFileDropTarget(() => getElement()?.getBoundingClientRect() ?? null, {
      onActiveChange: (active) => {
        dragActive = active;
      },
      onDrop,
    }).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        cleanup = unlisten;
      }
    });
    return () => {
      disposed = true;
      cleanup?.();
      dragActive = false;
    };
  });

  return {
    get dragActive() {
      return dragActive;
    },
  };
}
