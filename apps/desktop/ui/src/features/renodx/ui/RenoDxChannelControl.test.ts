/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import RenoDxChannelControlTestHost from './RenoDxChannelControl.test-host.svelte';

describe('RenoDxChannelControl accessibility', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(() => {
    vi.stubGlobal(
      'ResizeObserver',
      class {
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
    document.body.replaceChildren();
  });

  it('keeps tooltip behavior on the real toggle buttons without adding a group tab stop', async () => {
    component = mount(RenoDxChannelControlTestHost, { target });
    flushSync();

    const group = target.querySelector<HTMLElement>('[role="group"]');
    const buttons = [...target.querySelectorAll<HTMLButtonElement>('button')];
    const descriptionId = group?.getAttribute('aria-describedby');

    expect(group?.getAttribute('aria-label')).toBe('Release channel');
    expect(group?.getAttribute('tabindex')).toBeNull();
    expect(descriptionId).toBeTruthy();
    expect(document.getElementById(descriptionId ?? '')?.textContent).toBe(
      'Choose the release channel.',
    );
    expect(buttons).toHaveLength(2);
    expect(buttons.filter((button) => button.tabIndex === 0)).toHaveLength(1);

    buttons[0]?.focus();
    flushSync();

    await vi.waitFor(() => {
      const tooltip = document.body.querySelector('[role="tooltip"]');
      expect(tooltip, document.body.innerHTML).not.toBeNull();
      expect(tooltip?.textContent).toContain('Choose the release channel.');
      expect(buttons[0]?.getAttribute('aria-describedby')).toBe(tooltip?.id);
    });
  });
});
