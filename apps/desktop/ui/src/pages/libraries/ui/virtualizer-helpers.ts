import { tick } from 'svelte';

export type VirtualPaddingItem = {
  start: number;
  end: number;
};

type Virtualizer = {
  scrollToOffset: (offset: number, options?: { align?: 'start' }) => void;
  measure: () => void;
};

type ResetVirtualizerOptions = {
  viewport: HTMLElement | null;
  virtualizer: Virtualizer;
  resetId: number;
  resetKey: string;
  currentResetId: () => number;
  currentResetKey: () => string;
};

export function getTopVirtualPadding(items: readonly VirtualPaddingItem[]): number {
  if (items.length === 0) return 0;

  return items[0].start;
}

export function getBottomVirtualPadding(
  items: readonly VirtualPaddingItem[],
  totalSize: number,
): number {
  if (items.length === 0) return 0;

  const lastItem = items[items.length - 1];

  return Math.max(totalSize - lastItem.end, 0);
}

/**
 * Resets the virtualizer scroll offset and remeasures rows, but only if the
 * caller is still the latest reset request for the same key. Used after sort
 * changes or filter switches so the next render starts at the top.
 */
export async function resetVirtualizerAfterLayout(options: ResetVirtualizerOptions): Promise<void> {
  await tick();
  await nextAnimationFrame();

  if (options.resetId !== options.currentResetId()) return;
  if (options.resetKey !== options.currentResetKey()) return;
  if (options.viewport === null) return;

  options.viewport.scrollTo({ top: 0, left: 0 });
  options.virtualizer.scrollToOffset(0, { align: 'start' });
  options.virtualizer.measure();
}

function nextAnimationFrame(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      resolve();
    });
  });
}
