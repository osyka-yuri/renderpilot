/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { createNvapiViewRefresh } from './nvapi-view-refresh';

let visibilityState = 'visible';
let visibilityDescriptor: PropertyDescriptor | undefined;
let hasVisibilityDescriptor = false;
let view: ReturnType<typeof createNvapiViewRefresh> | undefined;

afterEach(() => {
  view?.destroy();
  view = undefined;
  if (hasVisibilityDescriptor && visibilityDescriptor) {
    Object.defineProperty(document, 'visibilityState', visibilityDescriptor);
  } else if (hasVisibilityDescriptor) {
    Reflect.deleteProperty(document, 'visibilityState');
  }
  visibilityDescriptor = undefined;
  hasVisibilityDescriptor = false;
});

function setVisibility(state: string): void {
  if (!hasVisibilityDescriptor) {
    visibilityDescriptor = Object.getOwnPropertyDescriptor(document, 'visibilityState');
    hasVisibilityDescriptor = true;
  }
  visibilityState = state;
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    get: () => visibilityState,
  });
}

describe('createNvapiViewRefresh', () => {
  it('uses the initial fetch, then refreshes a returned tab once after its read settles', () => {
    const refresh = vi.fn();
    view = createNvapiViewRefresh(refresh);

    view.sync('nvidia', true);
    expect(refresh).not.toHaveBeenCalled();
    view.sync(null, true);
    view.sync('nvidia', false);
    expect(refresh).not.toHaveBeenCalled();
    view.sync('nvidia', true);

    expect(refresh).toHaveBeenCalledOnce();
    expect(refresh).toHaveBeenCalledWith('nvidia');
  });

  it('coalesces focus while busy and cancels a request when the view exits', () => {
    const refresh = vi.fn();
    view = createNvapiViewRefresh(refresh);
    view.sync('nvidia', false);

    window.dispatchEvent(new Event('focus'));
    window.dispatchEvent(new Event('focus'));
    expect(refresh).not.toHaveBeenCalled();

    view.sync(null, true);
    expect(refresh).not.toHaveBeenCalled();
  });

  it('cancels a queued request when the game owner changes', () => {
    const refresh = vi.fn();
    view = createNvapiViewRefresh(refresh);
    view.sync('nvidia', true, 'game-a');
    view.sync(null, true, 'game-a');
    view.sync('nvidia', false, 'game-a');
    view.sync('nvidia', true, 'game-b');

    expect(refresh).not.toHaveBeenCalled();
  });

  it('waits while hidden and refreshes when the visible view regains focus', () => {
    setVisibility('hidden');
    const refresh = vi.fn();
    view = createNvapiViewRefresh(refresh);
    view.sync('nvidia', true);
    window.dispatchEvent(new Event('focus'));
    expect(refresh).not.toHaveBeenCalled();

    setVisibility('visible');
    window.dispatchEvent(new Event('focus'));
    expect(refresh).toHaveBeenCalledOnce();
  });

  it('keeps a refresh pending when a read settles while hidden until focus returns', () => {
    setVisibility('hidden');
    const refresh = vi.fn();
    view = createNvapiViewRefresh(refresh);
    view.sync(null, false);
    view.sync('nvidia', false);
    view.sync('nvidia', true);
    expect(refresh).not.toHaveBeenCalled();

    setVisibility('visible');
    window.dispatchEvent(new Event('focus'));
    expect(refresh).toHaveBeenCalledOnce();
  });
});
