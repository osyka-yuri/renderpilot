/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import OverlayScrollLockTestHost from './overlay-scroll-lock.test-host.svelte';

type TestHost = {
  setDialogOpen: (open: boolean) => void;
  setSelectOpen: (open: boolean) => void;
};

describe('overlay body scroll lock', () => {
  let target: HTMLDivElement;
  let component: TestHost | undefined;

  beforeEach(() => {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      },
    );
    Object.defineProperties(HTMLElement.prototype, {
      hasPointerCapture: {
        configurable: true,
        value: vi.fn(() => false),
      },
      releasePointerCapture: {
        configurable: true,
        value: vi.fn(),
      },
    });
    const nativeSetAttribute = document.body.setAttribute.bind(document.body);
    vi.spyOn(document.body, 'setAttribute').mockImplementation((name, value) => {
      // WebView2 150 updates the serialized attribute but can leave the live
      // CSSStyleDeclaration unchanged. A no-op reproduces the observable
      // cleanup failure: the scroll-lock declarations remain effective.
      if (name === 'style') {
        return;
      }
      nativeSetAttribute(name, value);
    });
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      component.setDialogOpen(false);
      component.setSelectOpen(false);
      flushSync();
      await settleOverlays();
      await unmount(component);
      component = undefined;
    }
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    delete (HTMLElement.prototype as Partial<HTMLElement>).hasPointerCapture;
    delete (HTMLElement.prototype as Partial<HTMLElement>).releasePointerCapture;
    document.body.replaceChildren();
    document.body.removeAttribute('style');
  });

  it.each([
    ['select', 'setSelectOpen'],
    ['dialog', 'setDialogOpen'],
  ] as const)('restores the body after the %s closes', async (_overlay, setOpen) => {
    component = mount(OverlayScrollLockTestHost, { target });
    flushSync();
    document.body.style.setProperty('--test-body-style', 'preserved');

    component[setOpen](true);
    flushSync();
    await tick();
    expect(document.body.style.pointerEvents).toBe('none');
    expect(document.body.style.overflow).toBe('hidden');

    component[setOpen](false);
    flushSync();
    await settleOverlays();

    expect(document.body.style.pointerEvents).toBe('');
    expect(document.body.style.overflow).toBe('');
    expect(document.body.style.getPropertyValue('--test-body-style')).toBe('preserved');
  });
});

async function settleOverlays(): Promise<void> {
  await tick();
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        resolve();
      });
    });
  });
  await new Promise<void>((resolve) => {
    setTimeout(resolve, 32);
  });
  await tick();
}
