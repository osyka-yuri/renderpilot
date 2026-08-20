/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import SettingsAppearanceSection from './SettingsAppearanceSection.svelte';

describe('SettingsAppearanceSection language loading state', () => {
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
    Object.defineProperties(HTMLElement.prototype, {
      hasPointerCapture: {
        configurable: true,
        value: vi.fn(() => false),
      },
      releasePointerCapture: {
        configurable: true,
        value: vi.fn(),
      },
    });
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      flushSync();
      await tick();
      await unmount(component);
      component = undefined;
    }
    vi.unstubAllGlobals();
    delete (HTMLElement.prototype as Partial<HTMLElement>).hasPointerCapture;
    delete (HTMLElement.prototype as Partial<HTMLElement>).releasePointerCapture;
    document.body.replaceChildren();
  });

  it('shows the pending language with a spinner without disabling the selector', () => {
    component = mount(SettingsAppearanceSection, {
      target,
      props: {
        languageMode: 'ru',
        languageBusy: true,
        languageOptions: [
          { value: 'en', label: 'English' },
          { value: 'ru', label: 'Русский' },
        ],
      },
    });
    flushSync();

    const trigger = target.querySelector<HTMLButtonElement>(
      'button[aria-label="Language"][aria-haspopup="listbox"]',
    );
    expect(trigger).not.toBeNull();
    expect(trigger?.disabled).toBe(false);
    expect(trigger?.getAttribute('aria-busy')).toBe('true');
    expect(trigger?.textContent).toContain('Русский');
    expect(trigger?.querySelector('[aria-hidden="true"].animate-spin')).not.toBeNull();
    expect(trigger?.querySelector('[role="status"]')).toBeNull();
  });
});
