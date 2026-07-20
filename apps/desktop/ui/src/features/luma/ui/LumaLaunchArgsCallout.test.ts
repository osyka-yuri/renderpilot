/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

vi.mock('svelte-sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import { hasKnownLaunchArgsInstructions } from '../model/launch-args';
import LumaLaunchArgsCalloutTestHost from './LumaLaunchArgsCallout.test-host.svelte';

const KNOWN_LAUNCHERS = ['Steam', 'Gog', 'Epic', 'Ea', 'Ubisoft'] as const;

describe('LumaLaunchArgsCallout', () => {
  let target: HTMLDivElement;
  let component: object | undefined;
  const writeText = vi.fn<Navigator['clipboard']['writeText']>();

  function render(launcher: string): void {
    component = mount(LumaLaunchArgsCalloutTestHost, {
      target,
      props: { launchArgs: ['-dx11'], launcher },
    });
    flushSync();
  }

  beforeEach(() => {
    target = document.createElement('div');
    document.body.append(target);
    writeText.mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
      component = undefined;
    }
    target.remove();
    vi.clearAllMocks();
  });

  it.each(KNOWN_LAUNCHERS)('shows the two-step instruction for %s', (launcher) => {
    expect(hasKnownLaunchArgsInstructions(launcher)).toBe(true);
    render(launcher);

    expect(target.textContent).toContain('This Luma profile requires DirectX 11');
    expect(target.textContent).toContain('Copy the required launch arguments:');
    expect(target.textContent).toContain('-dx11');
    // Store-specific guidance plus the neutral fallback path.
    expect(target.textContent).toMatch(/If you start the game through/);
    expect(target.textContent).toContain('Use the launch method that actually starts the game.');

    const copyButton = target.querySelector<HTMLButtonElement>('button');
    expect(copyButton).not.toBeNull();
    expect(copyButton?.getAttribute('aria-label')).toBe('Copy arguments');
  });

  it('only shows neutral instructions for an unknown launch method', () => {
    expect(hasKnownLaunchArgsInstructions('Manual')).toBe(false);
    render('Manual');

    expect(target.textContent).toContain('Use the launch method that actually starts the game.');
    expect(target.textContent).not.toContain('If you start the game through Steam');
  });

  it('copies the required argument', async () => {
    render('Steam');

    const copyButton = target.querySelector<HTMLButtonElement>('button');
    expect(copyButton).not.toBeNull();
    copyButton?.click();

    await vi.waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('-dx11');
      expect(copyButton?.getAttribute('aria-label')).toBe('Copied');
    });
  });
});
