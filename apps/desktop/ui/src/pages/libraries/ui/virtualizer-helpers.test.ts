/**
 * @vitest-environment jsdom
 */

import { afterEach, describe, expect, it, vi } from 'vitest';

import { resetVirtualizerAfterLayout } from './virtualizer-helpers';

describe('resetVirtualizerAfterLayout', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('resets and remeasures the active virtualizer after layout', async () => {
    const viewport = document.createElement('div');
    const scrollTo = vi.fn();
    const scrollToOffset = vi.fn();
    const measure = vi.fn();
    Object.defineProperty(viewport, 'scrollTo', { value: scrollTo });
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      callback(0);
      return 0;
    });

    await resetVirtualizerAfterLayout({
      viewport,
      virtualizer: { scrollToOffset, measure },
      resetId: 3,
      resetKey: 'nvidia:dlss:false:2',
      currentResetId: () => 3,
      currentResetKey: () => 'nvidia:dlss:false:2',
    });

    expect(scrollTo).toHaveBeenCalledWith({ top: 0, left: 0 });
    expect(scrollToOffset).toHaveBeenCalledWith(0, { align: 'start' });
    expect(measure).toHaveBeenCalledOnce();
  });
});
