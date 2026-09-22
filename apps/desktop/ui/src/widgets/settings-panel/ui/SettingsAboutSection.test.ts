/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import type { SettingsUpdateAction } from '@features/app-updater';
import SettingsAboutSection from './SettingsAboutSection.svelte';

describe('SettingsAboutSection', () => {
  let target: HTMLDivElement;
  let component: object | undefined;
  const onCheckForUpdates = vi.fn();

  function render({
    appVersion = '1.0.0',
    updateAction = 'check',
  }: {
    appVersion?: string | null;
    updateAction?: SettingsUpdateAction;
  } = {}): void {
    component = mount(SettingsAboutSection, {
      target,
      props: {
        appVersion,
        updateAction,
        onCheckForUpdates,
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

  it('renders check for updates button when idle and invokes callback', () => {
    render();

    const button = target.querySelector<HTMLButtonElement>('button');
    const status = target.querySelector('[role="status"]');

    expect(button?.textContent).toContain('Check for updates');
    expect(button?.disabled).toBe(false);
    expect(button?.getAttribute('aria-busy')).toBe('false');
    expect(button?.querySelector('svg')?.classList.contains('animate-spin')).toBe(false);
    expect(status?.textContent.trim()).toBe('');

    button?.click();
    expect(onCheckForUpdates).toHaveBeenCalledTimes(1);
  });

  it('indicates checking state with spinning icon, aria-busy, and live region', () => {
    render({ updateAction: 'checking' });

    const button = target.querySelector<HTMLButtonElement>('button');
    const status = target.querySelector('[role="status"]');

    expect(button?.textContent).toContain('Check for updates');
    expect(button?.disabled).toBe(true);
    expect(button?.getAttribute('aria-busy')).toBe('true');
    expect(button?.querySelector('svg')?.classList.contains('animate-spin')).toBe(true);
    expect(status?.textContent.trim()).toBe('Checking…');
  });

  it('indicates busy state during update execution', () => {
    render({ updateAction: 'busy' });

    const button = target.querySelector<HTMLButtonElement>('button');
    const status = target.querySelector('[role="status"]');

    expect(button?.textContent).toContain('Updating…');
    expect(button?.disabled).toBe(true);
    expect(button?.getAttribute('aria-busy')).toBe('true');
    expect(button?.querySelector('svg')?.classList.contains('animate-spin')).toBe(true);
    expect(status?.textContent.trim()).toBe('Updating…');
  });

  it('renders open-update action with distinct icon and cleared status', () => {
    render({ updateAction: 'open-update' });

    const button = target.querySelector<HTMLButtonElement>('button');
    const status = target.querySelector('[role="status"]');

    expect(button?.textContent).toContain('Update available');
    expect(button?.disabled).toBe(false);
    expect(button?.getAttribute('aria-busy')).toBe('false');
    expect(button?.querySelector('svg')?.classList.contains('animate-spin')).toBe(false);
    expect(status?.textContent.trim()).toBe('');
  });
});
