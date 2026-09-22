/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import ShellHeaderTestHost from './ShellHeader.test-host.svelte';

describe('ShellHeader refresh button', () => {
  let target: HTMLDivElement;
  let component: object | undefined;
  const onRefresh = vi.fn();

  function render({
    busy = false,
    refreshing = false,
  }: { busy?: boolean; refreshing?: boolean } = {}): void {
    component = mount(ShellHeaderTestHost, {
      target,
      props: {
        busy,
        refreshing,
        onRefresh,
      },
    });
    flushSync();
  }

  beforeEach(() => {
    vi.stubGlobal(
      'matchMedia',
      vi.fn((query: string) => ({
        matches: false,
        media: query,
        onchange: null,
        addListener: vi.fn(),
        removeListener: vi.fn(),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(() => false),
      })),
    );
    vi.stubGlobal(
      'ResizeObserver',
      class ResizeObserverMock {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      },
    );
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
      component = undefined;
    }
    vi.unstubAllGlobals();
    target.remove();
    vi.clearAllMocks();
  });

  it('renders refresh button in idle state and triggers onRefresh on click', () => {
    render();

    const button = target.querySelector<HTMLButtonElement>('button[aria-label="Refresh"]');
    const status = target.querySelector('[role="status"]');

    expect(button?.disabled).toBe(false);
    expect(button?.getAttribute('aria-busy')).toBe('false');
    expect(button?.querySelector('svg')?.classList.contains('animate-spin')).toBe(false);
    expect(status?.textContent.trim()).toBe('');

    button?.click();
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });

  it('indicates active refreshing state with aria-busy, spinning icon, and live-region status', () => {
    render({ busy: true, refreshing: true });

    const button = target.querySelector<HTMLButtonElement>('button[aria-label="Refresh"]');
    const status = target.querySelector('[role="status"]');

    expect(button?.disabled).toBe(true);
    expect(button?.getAttribute('aria-busy')).toBe('true');
    expect(button?.querySelector('svg')?.classList.contains('animate-spin')).toBe(true);
    expect(status?.textContent.trim()).toBe('Refreshing…');
  });
});
