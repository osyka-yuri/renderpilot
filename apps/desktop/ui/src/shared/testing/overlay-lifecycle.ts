import { expect, vi } from 'vitest';
import { flushSync, unmount } from 'svelte';

export type MountedComponent = Parameters<typeof unmount>[0];

/**
 * Close a Bits UI overlay while its jsdom document is still alive, wait for
 * its asynchronous body-scroll-lock cleanup, and only then unmount it.
 */
export async function closeAndUnmountBitsOverlay(
  component: MountedComponent,
  initialBodyStyle: string,
): Promise<void> {
  try {
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    flushSync();
    await waitForBodyStyle(initialBodyStyle);
  } finally {
    await unmount(component);
  }
}

async function waitForBodyStyle(expectedCssText: string): Promise<void> {
  await vi.waitFor(() => {
    expect(document.body.style.cssText).toBe(expectedCssText);
  });
}
