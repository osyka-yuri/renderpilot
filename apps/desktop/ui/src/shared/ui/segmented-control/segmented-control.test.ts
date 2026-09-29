/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import SegmentedControlTestHost from './segmented-control.test-host.svelte';

describe('SegmentedControl', () => {
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
  });

  it('keeps one selected item and supports arrow navigation', () => {
    component = mount(SegmentedControlTestHost, { target });
    flushSync();

    const [cards, list, disabled] = target.querySelectorAll<HTMLButtonElement>('[role="radio"]');
    const group = target.querySelector<HTMLElement>('[role="radiogroup"]');
    expect(group?.getAttribute('aria-label')).toBe('Catalog view');
    expect(group?.getAttribute('data-orientation')).toBe('horizontal');
    expect(cards.getAttribute('aria-checked')).toBe('true');

    cards.click();
    flushSync();
    expect(cards.getAttribute('aria-checked')).toBe('true');

    list.click();
    flushSync();
    expect(list.getAttribute('aria-checked')).toBe('true');
    expect(target.querySelector('output')?.textContent).toBe('list');

    list.focus();
    list.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
    flushSync();
    expect(document.activeElement).toBe(cards);
    expect(cards.getAttribute('aria-checked')).toBe('true');
    expect(target.querySelector('output')?.textContent).toBe('cards');
    disabled.click();
    flushSync();
    expect(target.querySelector('output')?.textContent).toBe('cards');

    cards.dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true }));
    flushSync();
    cards.dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true }));
    flushSync();
    expect(cards.getAttribute('aria-checked')).toBe('true');
    expect(disabled.disabled).toBe(true);
  });

  it('disables every item when the root is disabled', () => {
    component = mount(SegmentedControlTestHost, {
      target,
      props: { rootDisabled: true },
    });
    flushSync();

    const group = target.querySelector<HTMLElement>('[role="radiogroup"]');
    const items = target.querySelectorAll<HTMLButtonElement>('[role="radio"]');

    expect(group?.getAttribute('aria-disabled')).toBe('true');
    expect(items.length).toBe(3);
    for (const item of items) {
      expect(item.disabled).toBe(true);
    }

    items[1]?.click();
    flushSync();
    expect(target.querySelector('output')?.textContent).toBe('cards');
  });
});
