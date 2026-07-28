/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { createGameDetails } from '@entities/game';

import GameDetailsPageTestHost from './GameDetailsPage.test-host.svelte';

describe('GameDetailsPage navigation intent', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(() => {
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

  it('prefetches operations on pointer and keyboard intent before opening them', () => {
    const onPreloadOperations = vi.fn();
    const onOpenOperations = vi.fn();

    component = mount(GameDetailsPageTestHost, {
      target,
      props: {
        details: createGameDetails(),
        onOpenOperations,
        onPreloadOperations,
      },
    });
    flushSync();

    const operations = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent.trim() === 'Operations Journal',
    );
    expect(operations).toBeDefined();

    operations?.dispatchEvent(new Event('pointerenter'));
    operations?.focus();
    operations?.click();

    expect(onPreloadOperations).toHaveBeenCalledTimes(2);
    expect(onOpenOperations).toHaveBeenCalledTimes(1);
  });
});
