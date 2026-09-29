import { getContext, setContext } from 'svelte';

export type SegmentedControlSize = 'default' | 'sm';

export type SegmentedControlContext = {
  readonly size: SegmentedControlSize;
};

const SEGMENTED_CONTROL_CONTEXT_KEY = Symbol('segmented-control');

export function setSegmentedControlCtx(ctx: SegmentedControlContext): void {
  setContext(SEGMENTED_CONTROL_CONTEXT_KEY, ctx);
}

export function getSegmentedControlCtx(): SegmentedControlContext {
  const ctx = getContext<SegmentedControlContext | undefined>(SEGMENTED_CONTROL_CONTEXT_KEY);

  if (!ctx) {
    throw new Error('SegmentedControlItem must be used inside SegmentedControl.');
  }

  return ctx;
}
