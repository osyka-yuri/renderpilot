/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import AddonCardShellHost from './AddonCardShell.test-host.svelte';

describe('AddonCardShell', () => {
  let target: HTMLDivElement;
  let component: object | undefined;
  const onRetry = vi.fn();

  function render(
    props: {
      showLoading?: boolean;
      showLoadError?: boolean;
      showAttribution?: boolean;
      retrying?: boolean;
      actionsDisabled?: boolean;
      body?: string;
      headerLabel?: string | null;
    } = {},
  ): void {
    component = mount(AddonCardShellHost, {
      target,
      props: {
        onRetry,
        ...props,
      },
    });
    flushSync();
  }

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
    vi.clearAllMocks();
  });

  it('shows the loading label instead of body content', () => {
    render({ showLoading: true, body: 'hidden-body' });

    expect(target.textContent).toContain('Loading card…');
    expect(target.querySelector('[data-testid="shell-body"]')).toBeNull();
  });

  it('shows availability failure and retries', () => {
    render({ showLoadError: true });

    expect(target.textContent).toContain('Could not check');
    expect(target.querySelector('[data-testid="shell-body"]')).toBeNull();

    const retry = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent === 'Retry',
    );
    retry?.click();

    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('renders body content when neither loading nor load-error', () => {
    render({ body: 'ready-content', headerLabel: 'header-slot' });

    expect(target.querySelector('[data-testid="shell-body"]')?.textContent).toBe('ready-content');
    expect(target.querySelector('[data-testid="header-action"]')?.textContent).toBe('header-slot');
  });

  it('shows attribution in the footer when enabled', () => {
    render({ showAttribution: true });

    const link = target.querySelector<HTMLAnchorElement>('a[href="https://example.test/project"]');
    expect(link).not.toBeNull();
    expect(target.textContent).toContain('Luma Framework by Filoppi.');
  });

  it('hides attribution when disabled', () => {
    render({ showAttribution: false });

    expect(target.querySelector('a[href="https://example.test/project"]')).toBeNull();
    expect(target.textContent).not.toContain('Luma Framework by Filoppi.');
  });
});
