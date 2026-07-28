/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import DesktopShellTestHost from './DesktopShell.test-host.svelte';

describe('DesktopShell navigation intent', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(() => {
    Object.defineProperty(window, 'matchMedia', {
      configurable: true,
      value: vi.fn((query: string) => ({
        matches: false,
        media: query,
        onchange: null,
        addListener: vi.fn(),
        removeListener: vi.fn(),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(() => false),
      })),
    });
    Object.defineProperty(window, 'ResizeObserver', {
      configurable: true,
      value: class ResizeObserverMock {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      },
    });
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
      component = undefined;
    }
    target.remove();
  });

  it('prefetches sidebar pages on pointer and keyboard intent', () => {
    const onNavigate = vi.fn();
    const onPreload = vi.fn();

    component = mount(DesktopShellTestHost, {
      target,
      props: { onNavigate, onPreload },
    });
    flushSync();

    const buttons = [...target.querySelectorAll<HTMLButtonElement>('button')];
    const libraries = buttons.find((button) => button.textContent.trim() === 'Libraries');
    const settings = buttons.find((button) => button.textContent.trim() === 'Settings');

    expect(libraries).toBeDefined();
    expect(settings).toBeDefined();

    libraries?.dispatchEvent(new Event('pointerenter'));
    settings?.focus();

    expect(onPreload).toHaveBeenCalledWith('libraries');
    expect(onPreload).toHaveBeenCalledWith('settings');

    libraries?.click();
    expect(onNavigate).toHaveBeenCalledWith('libraries');
  });

  it('prefetches the details page from the operations breadcrumb', () => {
    const onNavigate = vi.fn();
    const onPreload = vi.fn();

    component = mount(DesktopShellTestHost, {
      target,
      props: {
        screen: 'operations',
        selectedGameTitle: 'Control',
        onNavigate,
        onPreload,
      },
    });
    flushSync();

    const detailsLink = [...target.querySelectorAll<HTMLAnchorElement>('a')].find(
      (link) => link.textContent.trim() === 'Control',
    );

    expect(detailsLink).toBeDefined();

    detailsLink?.dispatchEvent(new Event('pointerenter'));
    detailsLink?.focus();
    detailsLink?.click();

    expect(onPreload).toHaveBeenCalledTimes(2);
    expect(onPreload).toHaveBeenNthCalledWith(1, 'details');
    expect(onPreload).toHaveBeenNthCalledWith(2, 'details');
    expect(onNavigate).toHaveBeenCalledWith('details');
  });
});
