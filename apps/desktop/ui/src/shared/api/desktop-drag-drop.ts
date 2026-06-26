import { getCurrentWebview } from '@tauri-apps/api/webview';
import { isDesktopPreviewMode } from '@shared/api-preview';

export type FileDropTargetCallbacks = {
  /** Toggled as the drag enters/leaves the target's bounds, for hover styling. */
  onActiveChange?: (active: boolean) => void;
  /** Called with the dropped file paths when a drop lands inside the target. */
  onDrop: (paths: string[]) => void;
};

/**
 * Registers a webview-global Tauri drag-drop listener scoped to a single element.
 *
 * Tauri delivers OS file drops at the webview level (the only way to obtain real
 * filesystem paths — HTML5 `ondrop` cannot), with positions in **physical** pixels.
 * This hit-tests each position against `getRect()` (CSS pixels) after dividing by
 * `devicePixelRatio`, so the drop zone is correct on HiDPI displays and scoped to
 * the target rather than the whole window. Returns a cleanup that removes the
 * listener. A no-op in preview mode (returns a cleanup that does nothing).
 *
 * Relies on the default `dragDropEnabled: true`; if that is disabled the webview
 * would surface HTML5 drops instead and this listener would not fire.
 */
export async function createFileDropTarget(
  getRect: () => DOMRect | null,
  { onActiveChange, onDrop }: FileDropTargetCallbacks,
): Promise<() => void> {
  if (isDesktopPreviewMode()) {
    return () => undefined;
  }

  const isInside = (physicalX: number, physicalY: number): boolean => {
    const rect = getRect();
    if (!rect) return false;
    const dpr = window.devicePixelRatio || 1;
    const x = physicalX / dpr;
    const y = physicalY / dpr;
    return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
  };

  return getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === 'enter' || payload.type === 'over') {
      onActiveChange?.(isInside(payload.position.x, payload.position.y));
      return;
    }
    if (payload.type === 'leave') {
      onActiveChange?.(false);
      return;
    }
    // payload.type === 'drop'
    onActiveChange?.(false);
    if (isInside(payload.position.x, payload.position.y)) {
      onDrop(payload.paths);
    }
  });
}
