/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import { setLanguageMode } from '@shared/i18n';
import { closeAndUnmountBitsOverlay, type MountedComponent } from '@shared/testing';

import DesktopShellTestHost from './DesktopShell.test-host.svelte';

describe('DesktopShell navigation intent', () => {
  let target: HTMLDivElement;
  let component: MountedComponent | undefined;
  let initialBodyStyle: string;

  function mockViewport(isMobile: boolean): void {
    vi.stubGlobal(
      'matchMedia',
      vi.fn((query: string) => ({
        matches: isMobile,
        media: query,
        onchange: null,
        addListener: vi.fn(),
        removeListener: vi.fn(),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(() => false),
      })),
    );
  }

  function requireElement<T extends Element>(element: T | null, label: string): T {
    if (!element) {
      throw new Error(`Expected ${label} to be rendered`);
    }

    return element;
  }

  beforeEach(async () => {
    await setLanguageMode('en');
    mockViewport(false);
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
    initialBodyStyle = document.body.style.cssText;
  });

  afterEach(async () => {
    const mountedComponent = component;
    component = undefined;
    try {
      if (mountedComponent) {
        await closeAndUnmountBitsOverlay(mountedComponent, initialBodyStyle);
      }
    } finally {
      target.remove();
      document.body.replaceChildren();
      vi.restoreAllMocks();
      vi.unstubAllGlobals();
    }
  });

  it('does not move focus on initial mount or on locale and title-only updates', async () => {
    const focusOrigin = document.createElement('button');
    document.body.append(focusOrigin);
    focusOrigin.focus();

    component = mount(DesktopShellTestHost, {
      target,
      props: { screen: 'details', onNavigate: vi.fn(), onPreload: vi.fn() },
    });
    flushSync();
    await tick();

    expect(document.activeElement).toBe(focusOrigin);

    await setLanguageMode('ru');
    flushSync();
    await tick();
    expect(document.activeElement).toBe(focusOrigin);

    target.querySelector<HTMLButtonElement>('[data-test-action="rename-game"]')?.click();
    flushSync();
    await tick();
    expect(document.activeElement).toBe(focusOrigin);

    focusOrigin.remove();
  });

  it('keeps the document title aligned with the game, route, and locale', async () => {
    component = mount(DesktopShellTestHost, {
      target,
      props: {
        screen: 'details',
        selectedGameTitle: 'Control',
        onNavigate: vi.fn(),
        onPreload: vi.fn(),
      },
    });
    flushSync();
    await tick();

    expect(document.title).toBe('Control — RenderPilot');

    target.querySelector<HTMLButtonElement>('[data-test-action="rename-game"]')?.click();
    flushSync();
    await tick();
    expect(document.title).toBe('Renamed Test Game — RenderPilot');

    const settings = [...target.querySelectorAll<HTMLAnchorElement>('nav a')].find(
      (link) => link.textContent.trim() === 'Settings',
    );
    settings?.click();
    flushSync();
    await tick();
    expect(document.title).toBe('Settings — RenderPilot');

    await setLanguageMode('ru');
    flushSync();
    await tick();
    expect(document.title).toBe('Настройки — RenderPilot');
  });

  it('moves focus to the main landmark after a screen transition', async () => {
    component = mount(DesktopShellTestHost, {
      target,
      props: { onNavigate: vi.fn(), onPreload: vi.fn() },
    });
    flushSync();

    const libraries = [...target.querySelectorAll<HTMLAnchorElement>('nav a')].find(
      (link) => link.textContent.trim() === 'Libraries',
    );
    libraries?.click();
    await tick();
    await tick();

    const main = target.querySelector<HTMLElement>('#main-content');
    expect(main?.getAttribute('aria-label')).toBe('Libraries');
    expect(document.activeElement).toBe(main);
  });

  it('invalidates a stale focus tick when transitions happen rapidly', async () => {
    component = mount(DesktopShellTestHost, {
      target,
      props: { onNavigate: vi.fn(), onPreload: vi.fn() },
    });
    flushSync();

    const main = requireElement(
      target.querySelector<HTMLElement>('#main-content'),
      'main landmark',
    );
    const focusSpy = vi.spyOn(main, 'focus');
    const links = [...target.querySelectorAll<HTMLAnchorElement>('nav a')];
    links.find((link) => link.textContent.trim() === 'Libraries')?.click();
    links.find((link) => link.textContent.trim() === 'Settings')?.click();
    await tick();
    await tick();

    expect(focusSpy).toHaveBeenCalledOnce();
    expect(main.getAttribute('aria-label')).toBe('Settings');
    expect(document.activeElement).toBe(main);
  });

  it('cancels a pending transition focus when the shell unmounts', async () => {
    component = mount(DesktopShellTestHost, {
      target,
      props: { onNavigate: vi.fn(), onPreload: vi.fn() },
    });
    flushSync();

    const focusSpy = vi.spyOn(HTMLElement.prototype, 'focus');
    [...target.querySelectorAll<HTMLAnchorElement>('nav a')]
      .find((link) => link.textContent.trim() === 'Libraries')
      ?.click();
    await unmount(component);
    component = undefined;
    await tick();

    expect(focusSpy).not.toHaveBeenCalled();
  });

  it('prefetches sidebar pages on pointer and keyboard intent', () => {
    const onNavigate = vi.fn();
    const onPreload = vi.fn();

    component = mount(DesktopShellTestHost, {
      target,
      props: { onNavigate, onPreload },
    });
    flushSync();

    const links = [...target.querySelectorAll<HTMLAnchorElement>('nav a')];
    const libraries = links.find((link) => link.textContent.trim() === 'Libraries');
    const settings = links.find((link) => link.textContent.trim() === 'Settings');

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

  it('distinguishes the current page from its parent navigation location', () => {
    component = mount(DesktopShellTestHost, {
      target,
      props: {
        screen: 'operations',
        selectedGameTitle: 'Control',
        onNavigate: vi.fn(),
        onPreload: vi.fn(),
      },
    });
    flushSync();

    const primaryNavigation = target.querySelector('nav');
    const games = [...(primaryNavigation?.querySelectorAll<HTMLAnchorElement>('a') ?? [])].find(
      (link) => link.textContent.trim() === 'Games',
    );
    const main = target.querySelector('main');

    expect(games?.getAttribute('aria-current')).toBe('location');
    expect(main?.getAttribute('aria-label')).toBe('Journal');
  });

  it('closes mobile navigation before focus enters the named destination landmark', async () => {
    mockViewport(true);
    const onNavigate = vi.fn();

    component = mount(DesktopShellTestHost, {
      target,
      props: { onNavigate, onPreload: vi.fn() },
    });
    flushSync();

    target.querySelector<HTMLButtonElement>('[data-sidebar="trigger"]')?.click();
    flushSync();

    const dialog = document.body.querySelector<HTMLElement>('[role="dialog"][data-state="open"]');
    const libraries = [...(dialog?.querySelectorAll<HTMLAnchorElement>('a') ?? [])].find(
      (link) => link.textContent.trim() === 'Libraries',
    );

    expect(dialog).not.toBeNull();
    expect(libraries).toBeDefined();

    const main = requireElement(
      target.querySelector<HTMLElement>('#main-content'),
      'main landmark',
    );
    const nativeFocus = main.focus.bind(main);
    const dialogStatesAtFocus: boolean[] = [];
    vi.spyOn(main, 'focus').mockImplementation((options?: FocusOptions) => {
      dialogStatesAtFocus.push(
        document.body.querySelector('[role="dialog"][data-state="open"]') !== null,
      );
      nativeFocus(options);
    });

    libraries?.click();
    flushSync();
    await tick();
    await tick();

    expect(onNavigate).toHaveBeenCalledWith('libraries');
    expect(document.body.querySelector('[role="dialog"][data-state="open"]')).toBeNull();
    expect(main.getAttribute('aria-label')).toBe('Libraries');
    expect(document.activeElement).toBe(main);
    expect(dialogStatesAtFocus).toEqual([false]);
  });
});
