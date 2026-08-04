/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import LazyPageTestHost from './LazyPage.test-host.svelte';
import TestPage from './LazyPage.test-page.svelte';
import { createLazyPageResource } from './lazy-page-resource.svelte';

describe('LazyPage', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(() => {
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
      component = undefined;
    }
    target.remove();
    vi.restoreAllMocks();
  });

  it('renders a status while loading and then renders the typed page snippet', async () => {
    const pendingPage = Promise.withResolvers<typeof TestPage>();
    const page = createLazyPageResource({
      id: 'settings',
      loader: () => pendingPage.promise,
    });

    component = mount(LazyPageTestHost, {
      target,
      props: { page, onBack: vi.fn() },
    });
    flushSync();

    expect(target.querySelector('[role="status"]')?.textContent).toContain('Loading page');

    pendingPage.resolve(TestPage);
    await vi.waitFor(() => {
      flushSync();
      expect(target.querySelector('[data-testid="loaded-page"]')?.textContent).toBe(
        'Loaded page content',
      );
    });
  });

  it('shows an inline failure, retries, and can return to Games', async () => {
    const error = new Error('chunk missing');
    const loader = vi
      .fn<() => Promise<typeof TestPage>>()
      .mockRejectedValueOnce(error)
      .mockResolvedValueOnce(TestPage);
    const onBack = vi.fn();
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const page = createLazyPageResource({ id: 'libraries', loader });

    component = mount(LazyPageTestHost, {
      target,
      props: { page, onBack },
    });

    await vi.waitFor(() => {
      flushSync();
      expect(target.querySelector('[role="alert"]')).not.toBeNull();
    });

    const buttons = [...target.querySelectorAll<HTMLButtonElement>('button')];
    buttons.find((button) => button.textContent.includes('Back to Games'))?.click();
    expect(onBack).toHaveBeenCalledOnce();

    buttons.find((button) => button.textContent.includes('Try again'))?.click();
    await vi.waitFor(() => {
      flushSync();
      expect(target.querySelector('[data-testid="loaded-page"]')).not.toBeNull();
    });
    expect(loader).toHaveBeenCalledTimes(2);
  });
});
