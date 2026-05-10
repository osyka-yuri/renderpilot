/**
 * Why popover open-state changed.
 * - `programmatic`: caller requested open/close explicitly.
 * - `outside-pointer`: dismissed by pointer interaction outside registered boundaries.
 * - `escape`: dismissed by Escape on the active top-most layer.
 */
export type PopoverOpenChangeReason = 'programmatic' | 'outside-pointer' | 'escape';

/** Structured payload emitted by `Popover` on requested open-state change. */
export type PopoverOpenChangeEvent = {
  /** Requested next state. */
  open: boolean;
  /** Dismiss/open intent source. */
  reason: PopoverOpenChangeReason;
  /** Source DOM event for dismiss paths when available. */
  originalEvent?: PointerEvent | KeyboardEvent;
};
