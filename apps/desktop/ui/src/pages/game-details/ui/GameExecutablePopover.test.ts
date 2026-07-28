/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import type { ExecutableCandidate } from '@features/nvapi-settings';

import type { GameExecutableContext } from '../model/create-game-executable-context.svelte';
import type { ExecutableLockReason } from '../model/game-executable-lock';
import GameExecutablePopoverTestHost from './GameExecutablePopover.test-host.svelte';

describe('GameExecutablePopover', () => {
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
      await settleOverlays();
      await unmount(component);
      component = undefined;
    }
    vi.unstubAllGlobals();
    delete (HTMLElement.prototype as Partial<HTMLElement>).hasPointerCapture;
    delete (HTMLElement.prototype as Partial<HTMLElement>).releasePointerCapture;
    document.body.replaceChildren();
  });

  it('presents a focusable managed lock with rollback guidance', async () => {
    const trigger = render({ lockReason: 'd3d12_managed' });

    expect(trigger.disabled).toBe(false);
    expect(trigger.getAttribute('aria-disabled')).toBe('true');
    expect(trigger.getAttribute('aria-label')).toBe('Game executable: game.exe');
    expect(trigger.getAttribute('title')).toBeNull();
    expect(trigger.querySelector('.lucide-lock-keyhole')).not.toBeNull();

    trigger.focus();
    flushSync();

    await vi.waitFor(() => {
      const tooltip = document.body.querySelector<HTMLElement>('[role="tooltip"]');
      expect(tooltip?.textContent).toContain('Executable selection is locked');
      expect(tooltip?.textContent).toContain(
        'To choose a different EXE, fully roll back the D3D12 component.',
      );
    });

    trigger.click();
    flushSync();
    expect(document.body.querySelector(openPopoverSelector)).toBeNull();
  });

  it('uses recovery guidance for a repair-required lock', async () => {
    const trigger = render({ lockReason: 'd3d12_repair_required' });

    trigger.focus();
    flushSync();

    await vi.waitFor(() => {
      expect(document.body.querySelector('[role="tooltip"]')?.textContent).toContain(
        'Follow the recovery steps in the D3D12 card, then scan the game again.',
      );
    });
  });

  it('keeps the unlocked selector interactive without a native title', async () => {
    const trigger = render();

    expect(trigger.getAttribute('aria-disabled')).toBeNull();
    expect(trigger.getAttribute('title')).toBeNull();
    expect(trigger.querySelector('.lucide-chevron-down')).not.toBeNull();

    trigger.dispatchEvent(new MouseEvent('pointerenter'));
    flushSync();
    await vi.waitFor(() => {
      expect(document.body.querySelector('[role="tooltip"]')?.textContent).toContain(
        'Game executable: auto-detected.',
      );
    });

    trigger.blur();
    await openPopover(trigger);
    expect(popoverContent().textContent).toContain('Game executable');
  });

  it('renders candidate groups in order and applies the selected executable', async () => {
    const setOverride = vi.fn(() => Promise.resolve());
    const exe = executableContext({
      supportedCandidates: [
        executableCandidate('game.exe', 'game.exe', null),
        executableCandidate('alternate.exe', 'bin/alternate.exe', null),
      ],
      filteredOutCandidates: [
        executableCandidate('launcher.exe', 'launcher.exe', 'known_launcher'),
      ],
      setOverride,
    });
    const trigger = render({ exe });

    await openPopover(trigger);
    const content = popoverContent();
    const text = content.textContent;
    expect(text.indexOf('Detected game executables')).toBeLessThan(text.indexOf('Other'));

    const alternate = findButton(content, 'alternate.exe');
    alternate.click();
    flushSync();

    expect(setOverride).toHaveBeenCalledOnce();
    expect(setOverride).toHaveBeenCalledWith('steam:123', 'C:/Games/Test/bin/alternate.exe');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
  });

  it('resets a manual executable directly', async () => {
    const clearOverride = vi.fn(() => Promise.resolve());
    const exe = executableContext({
      effectiveExe: 'custom.exe',
      effectiveExeSource: 'override',
      clearOverride,
    });
    const trigger = render({ exe });

    await openPopover(trigger);
    const content = popoverContent();
    const resetButton = findButton(content, 'Reset to auto-detect');

    resetButton.click();
    flushSync();

    expect(clearOverride).toHaveBeenCalledOnce();
    expect(clearOverride).toHaveBeenCalledWith('steam:123');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
  });

  function render({
    exe = executableContext(),
    lockReason = null,
  }: {
    exe?: GameExecutableContext;
    lockReason?: ExecutableLockReason | null;
  } = {}): HTMLButtonElement {
    component = mount(GameExecutablePopoverTestHost, {
      target,
      props: {
        gameId: 'steam:123',
        exe,
        lockReason,
      },
    });
    flushSync();

    const trigger = target.querySelector<HTMLButtonElement>(
      'button[aria-label="Game executable: game.exe"], button[aria-label="Game executable: custom.exe"]',
    );
    if (!trigger) {
      throw new Error('Executable selector trigger was not rendered');
    }
    return trigger;
  }
});

async function openPopover(trigger: HTMLButtonElement): Promise<void> {
  trigger.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 }));
  trigger.click();
  flushSync();

  await vi.waitFor(() => {
    expect(document.body.querySelector(openPopoverSelector)).not.toBeNull();
  });
}

const openPopoverSelector = '[role="dialog"]';

function popoverContent(): HTMLElement {
  const content = document.body.querySelector<HTMLElement>(openPopoverSelector);
  if (!content) {
    throw new Error('Executable selector popover is not open');
  }
  return content;
}

function findButton(container: HTMLElement, text: string): HTMLButtonElement {
  const button = [...container.querySelectorAll<HTMLButtonElement>('button')].find((candidate) =>
    candidate.textContent.includes(text),
  );
  if (!button) {
    throw new Error(`Button containing "${text}" was not found`);
  }
  return button;
}

async function settleOverlays(): Promise<void> {
  await tick();
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        resolve();
      });
    });
  });
  await tick();
}

function executableContext(overrides: Partial<GameExecutableContext> = {}): GameExecutableContext {
  return {
    busy: false,
    loadError: null,
    effectiveExe: 'game.exe',
    effectiveExeSource: 'auto',
    supportedCandidates: [],
    filteredOutCandidates: [],
    reload: vi.fn(() => Promise.resolve()),
    clear: vi.fn(),
    setOverride: vi.fn(() => Promise.resolve()),
    clearOverride: vi.fn(() => Promise.resolve()),
    ...overrides,
  };
}

function executableCandidate(
  fileName: string,
  relativePath: string,
  rejection: string | null,
): ExecutableCandidate {
  return {
    relative_path: relativePath,
    file_name: fileName,
    absolute_path: `C:/Games/Test/${relativePath}`,
    size_bytes: 1,
    depth: relativePath.split('/').length - 1,
    rank_score: 0,
    rejection,
    rejection_token: rejection,
  };
}
