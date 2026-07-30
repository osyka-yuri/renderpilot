/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import SettingsAppearanceSection from './SettingsAppearanceSection.svelte';

describe('SettingsAppearanceSection', () => {
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
    expect(trigger?.querySelector('[role="status"]')).not.toBeNull();
  });

  it('keeps document pointer interactions available while the selector is open', async () => {
    component = mount(SettingsAppearanceSection, {
      target,
      props: {
        languageMode: 'en',
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
    if (!trigger) {
      throw new Error('Language selector trigger was not rendered');
    }
    trigger.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 }));
    trigger.click();
    flushSync();

    await vi.waitFor(() => {
      expect(document.body.querySelector('[role="listbox"]')).not.toBeNull();
    });
    await tick();
    await new Promise<void>((resolve) => {
      requestAnimationFrame(() => {
        resolve();
      });
    });
    expect(document.body.style.pointerEvents).not.toBe('none');
  });
});
