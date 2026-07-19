/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import AddonAvailabilityFailure from './AddonAvailabilityFailure.svelte';

describe('AddonAvailabilityFailure', () => {
  let target: HTMLDivElement;
  let component: object | undefined;
  const onRetry = vi.fn();

  function render({
    disabled = false,
    retrying = false,
  }: { disabled?: boolean; retrying?: boolean } = {}): void {
    component = mount(AddonAvailabilityFailure, {
      target,
      props: {
        disabled,
        retrying,
        onRetry,
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

  it('shows one shared error and retries', () => {
    render();

    const alert = target.querySelector('[role="alert"]');
    const retry = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent === 'Retry',
    );

    expect(alert?.textContent).toContain('Could not check');
    expect(retry?.disabled).toBe(false);

    retry?.click();

    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('keeps retry unavailable while another operation is active', () => {
    render({ disabled: true });

    const retry = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent === 'Retry',
    );

    expect(retry?.disabled).toBe(true);
    retry?.click();
    expect(onRetry).not.toHaveBeenCalled();
  });

  it('shows retry progress inside the disabled button', () => {
    render({ retrying: true });

    const retry = [...target.querySelectorAll<HTMLButtonElement>('button')].find((button) =>
      button.textContent.includes('Checking…'),
    );

    expect(retry?.disabled).toBe(true);
    expect(retry?.getAttribute('aria-busy')).toBe('true');
  });
});
