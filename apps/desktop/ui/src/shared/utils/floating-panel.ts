export const VIEWPORT_MARGIN = 8;
export const FLOATING_PANEL_GAP = 6;

type FloatingPanelLayoutOptions = {
  viewportMargin?: number;
  gap?: number;
  zIndex?: string;
};

const DEFAULT_LAYOUT_OPTIONS: Required<FloatingPanelLayoutOptions> = {
  viewportMargin: VIEWPORT_MARGIN,
  gap: FLOATING_PANEL_GAP,
  zIndex: '50',
};

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

function getElementSize(element: HTMLElement): { width: number; height: number } {
  const rect = element.getBoundingClientRect();

  return {
    width: rect.width || element.offsetWidth,
    height: rect.height || element.offsetHeight,
  };
}

function prepareFixedPanel(panel: HTMLElement, zIndex: string): void {
  Object.assign(panel.style, {
    position: 'fixed',
    zIndex,
    margin: '0',
    right: 'auto',
    bottom: 'auto',
  });
}

/**
 * Positions the floating panel below or above the trigger,
 * keeping it within the viewport whenever possible.
 */
export function layoutFloatingPanel(
  trigger: HTMLElement,
  panel: HTMLElement,
  options: FloatingPanelLayoutOptions = {},
): void {
  const { viewportMargin, gap, zIndex } = {
    ...DEFAULT_LAYOUT_OPTIONS,
    ...options,
  };

  prepareFixedPanel(panel, zIndex);

  const triggerRect = trigger.getBoundingClientRect();
  const { width: panelWidth, height: panelHeight } = getElementSize(panel);

  const viewportWidth = window.innerWidth;
  const viewportHeight = window.innerHeight;

  const minLeft = viewportMargin;
  const maxLeft = Math.max(viewportMargin, viewportWidth - viewportMargin - panelWidth);

  const preferredLeft = triggerRect.right - panelWidth;
  const left = clamp(preferredLeft, minLeft, maxLeft);

  const belowTop = triggerRect.bottom + gap;
  const aboveTop = triggerRect.top - panelHeight - gap;

  const fitsBelow = belowTop + panelHeight <= viewportHeight - viewportMargin;

  const preferredTop = fitsBelow ? belowTop : aboveTop;

  const minTop = viewportMargin;
  const maxTop = Math.max(viewportMargin, viewportHeight - viewportMargin - panelHeight);

  const top = clamp(preferredTop, minTop, maxTop);

  panel.style.top = `${Math.round(top)}px`;
  panel.style.left = `${Math.round(left)}px`;
}

export function attachFloatingPanelResizeScroll(schedule: () => void): () => void {
  const options: AddEventListenerOptions = { passive: true };

  window.addEventListener('resize', schedule, options);
  window.addEventListener('scroll', schedule, {
    ...options,
    capture: true,
  });

  return (): void => {
    window.removeEventListener('resize', schedule, options);
    window.removeEventListener('scroll', schedule, {
      capture: true,
    });
  };
}
