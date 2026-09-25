/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import type { ExecutableCandidate } from '@features/nvapi-settings';

import type { GameExecutableContext } from '../model/create-game-executable-context.svelte';
import type {
  ExecutableLockReason,
  ProfileSelectionBlockReason,
} from '../model/game-executable-lock';
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
    const host = component as {
      setProfileSelectionBlockReason: (reason: ProfileSelectionBlockReason | null) => void;
    };
    host.setProfileSelectionBlockReason('checking');
    flushSync();

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
      expect(tooltip?.textContent).not.toContain('Checking the NVIDIA profile.');
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

  it('uses profile-check copy without a D3D12 heading and preserves D3D12 precedence', async () => {
    render();
    const host = component as {
      setProfileSelectionBlockReason: (reason: ProfileSelectionBlockReason | null) => void;
    };
    host.setProfileSelectionBlockReason('checking');
    flushSync();
    const trigger = target.querySelector<HTMLButtonElement>(
      'button[aria-label="Game executable: game.exe"]',
    );
    if (!trigger) {
      throw new Error('Executable selector was not rendered while profile verification is pending');
    }

    trigger.focus();
    flushSync();
    await vi.waitFor(() => {
      const tooltip =
        document.body.querySelector<HTMLElement>('[role="tooltip"]')?.textContent ?? '';
      expect(tooltip).toContain('Checking the NVIDIA profile.');
      expect(tooltip).not.toContain('D3D12');
    });

    host.setProfileSelectionBlockReason('unverified');
    flushSync();
    await vi.waitFor(() => {
      const tooltip =
        document.body.querySelector<HTMLElement>('[role="tooltip"]')?.textContent ?? '';
      expect(tooltip).toContain('Could not verify the NVIDIA profile.');
      expect(tooltip).not.toContain('D3D12');
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

  it('blocks executable changes while profile ownership is unverified and re-enables after resolution', async () => {
    const setOverride = vi.fn(() => Promise.resolve(true));
    const clearOverride = vi.fn(() => Promise.resolve(true));
    const trigger = render({
      exe: executableContext({
        effectiveExeSource: 'override',
        autoAbsolutePath: 'C:/Games/Test/auto.exe',
        supportedCandidates: [executableCandidate('alternate.exe', 'bin/alternate.exe', null)],
        setOverride,
        clearOverride,
      }),
    });
    const host = component as {
      setProfileSelectionBlockReason: (reason: ProfileSelectionBlockReason | null) => void;
    };

    await openPopover(trigger);
    const popover = popoverContent();
    const alternate = findOption(popover, 'alternate.exe');
    const reset = findButton(popover, 'Reset to auto-detect');

    host.setProfileSelectionBlockReason('unverified');
    flushSync();
    const blockedTrigger = target.querySelector<HTMLButtonElement>(
      'button[aria-label="Game executable: game.exe"]',
    );
    if (!blockedTrigger) {
      throw new Error('Executable selector was not rendered while profile verification is blocked');
    }
    expect(blockedTrigger.getAttribute('aria-disabled')).toBe('true');
    expect(blockedTrigger.getAttribute('aria-label')).toBe('Game executable: game.exe');
    expect(blockedTrigger.getAttribute('title')).toBeNull();
    blockedTrigger.focus();
    flushSync();
    await vi.waitFor(() => {
      expect(document.body.querySelector('[role="tooltip"]')?.textContent).toContain(
        'Could not verify the NVIDIA profile.',
      );
      expect(document.body.querySelector('[role="tooltip"]')?.textContent).not.toContain('D3D12');
    });
    expect(document.body.querySelector(openPopoverSelector)).toBeNull();
    alternate.click();
    reset.click();
    blockedTrigger.click();
    flushSync();
    expect(setOverride).not.toHaveBeenCalled();
    expect(clearOverride).not.toHaveBeenCalled();
    expect(document.body.querySelector(openPopoverSelector)).toBeNull();

    host.setProfileSelectionBlockReason(null);
    flushSync();
    const resolvedTrigger = target.querySelector<HTMLButtonElement>(
      'button[aria-label="Game executable: game.exe"]',
    );
    if (!resolvedTrigger) {
      throw new Error('Executable selector was not rendered after profile verification');
    }
    expect(resolvedTrigger.getAttribute('aria-disabled')).toBeNull();
    await openPopover(resolvedTrigger);
    findOption(popoverContent(), 'alternate.exe').click();
    flushSync();
    expect(setOverride).toHaveBeenCalledWith('steam:123', 'C:/Games/Test/bin/alternate.exe');
  });

  it('renders candidate groups in order and applies the selected executable', async () => {
    const setOverride = vi.fn(() => Promise.resolve(true));
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
    expect(
      [...content.querySelectorAll('[role="group"]')].map((group) => group.textContent),
    ).toEqual([
      expect.stringContaining('Detected game executables'),
      expect.stringContaining('Other'),
    ]);

    const alternate = findOption(content, 'alternate.exe');
    alternate.click();
    flushSync();

    expect(setOverride).toHaveBeenCalledOnce();
    expect(setOverride).toHaveBeenCalledWith('steam:123', 'C:/Games/Test/bin/alternate.exe');
    await vi.waitFor(() => {
      expect(trigger.getAttribute('aria-expanded')).toBe('false');
    });
  });

  it('displays tooltip on keyboard focus of candidate option and ignores non-keyboard focus', async () => {
    const exe = executableContext({
      supportedCandidates: [
        executableCandidate('game.exe', 'game.exe', null),
        executableCandidate('alternate.exe', 'bin/alternate.exe', null),
      ],
    });
    const trigger = render({ exe });

    await openPopover(trigger);
    const content = popoverContent();
    const alternateLabel = findOption(content, 'alternate.exe');
    const radioItem = alternateLabel.querySelector<HTMLButtonElement>('[role="radio"]');
    if (!radioItem) {
      throw new Error('Radio item not found');
    }

    // 1. Programmatic / non-keyboard focus does not trigger tooltip
    radioItem.focus();
    flushSync();
    expect(document.body.querySelector('[role="tooltip"]')).toBeNull();

    // 2. Keyboard focus (with :focus-visible) triggers tooltip
    vi.spyOn(radioItem, 'matches').mockImplementation(function (this: Element, selector: string) {
      if (selector === ':focus-visible') {
        return document.activeElement === this;
      }
      return Element.prototype.matches.call(this, selector);
    });

    radioItem.blur();
    flushSync();
    radioItem.focus();
    flushSync();

    await vi.waitFor(() => {
      const tooltip = document.body.querySelector<HTMLElement>('[role="tooltip"]');
      expect(tooltip).not.toBeNull();
      expect(tooltip?.textContent).toContain('alternate.exe');
      if (tooltip?.id) {
        expect(radioItem.getAttribute('aria-describedby')).toBe(tooltip.id);
        expect(alternateLabel.getAttribute('aria-describedby')).toBeNull();
      }
    });

    // 3. Blur dismisses tooltip
    radioItem.blur();
    flushSync();
    await vi.waitFor(() => {
      expect(document.body.querySelector('[role="tooltip"]')).toBeNull();
    });
  });

  it('keeps the selector open and presents an accessible error when the selection fails', async () => {
    const exe = executableContext({
      supportedCandidates: [executableCandidate('alternate.exe', 'bin/alternate.exe', null)],
      changeError: 'The executable selection could not be saved.',
      setOverride: vi.fn(() => Promise.resolve(false)),
    });
    const trigger = render({ exe });

    await openPopover(trigger);
    findOption(popoverContent(), 'alternate.exe').click();
    flushSync();

    await vi.waitFor(() => {
      expect(trigger.getAttribute('aria-expanded')).toBe('true');
      expect(popoverContent().querySelector('[role="alert"]')?.textContent).toContain(
        'Could not update the executable selection.',
      );
      expect(popoverContent().querySelector('[role="alert"]')?.textContent).toContain(
        'The executable selection could not be saved.',
      );
    });
  });

  it('presents committed selection refresh failures separately from mutation failures', async () => {
    const trigger = render({
      exe: executableContext({
        refreshError: 'The NVIDIA profile details could not be refreshed.',
      }),
    });

    await openPopover(trigger);
    const content = popoverContent();
    const refreshStatus = content.querySelector('[role="status"]');

    expect(refreshStatus?.textContent).toContain(
      'Executable selection was updated, but related game details could not be refreshed.',
    );
    expect(refreshStatus?.textContent).toContain(
      'The NVIDIA profile details could not be refreshed.',
    );
    expect(content.querySelector('[role="alert"]')).toBeNull();
  });

  it('resets a manual executable directly', async () => {
    const clearOverride = vi.fn(() => Promise.resolve(true));
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
    await vi.waitFor(() => {
      expect(trigger.getAttribute('aria-expanded')).toBe('false');
    });
  });

  it('keeps the confirmed move dialog and destination after a failed move', async () => {
    const moveProfile = vi.fn(() => Promise.resolve(false));
    const trigger = render({
      exe: executableContext({
        supportedCandidates: [executableCandidate('alternate.exe', 'bin/alternate.exe', null)],
      }),
      ownedBindingPath: 'C:/Games/Test/game.exe',
      onMoveProfile: moveProfile,
    });

    await openPopover(trigger);
    findOption(popoverContent(), 'alternate.exe').click();
    flushSync();

    const confirmDialog = await waitForMoveDialog();
    expect(confirmDialog.textContent).toContain('C:/Games/Test/bin/alternate.exe');
    findButton(confirmDialog, 'Move profile').click();

    await vi.waitFor(() => {
      expect(moveProfile).toHaveBeenCalledWith('C:/Games/Test/bin/alternate.exe', false);
      expect(confirmDialog.querySelector('[role="alert"]')?.textContent).toContain(
        'Could not move the NVIDIA profile.',
      );
    });
    expect(document.body.querySelector(openPopoverSelector)).toBe(confirmDialog);
    expect(confirmDialog.textContent).toContain('C:/Games/Test/bin/alternate.exe');
  });

  it('keeps a running move confirmation visible while the profile status refreshes', async () => {
    const moveResult = Promise.withResolvers<boolean>();
    const moveProfile = vi.fn(() => moveResult.promise);
    const trigger = render({
      exe: executableContext({
        supportedCandidates: [executableCandidate('alternate.exe', 'bin/alternate.exe', null)],
      }),
      ownedBindingPath: 'C:/Games/Test/game.exe',
      onMoveProfile: moveProfile,
    });

    await openPopover(trigger);
    findOption(popoverContent(), 'alternate.exe').click();
    flushSync();

    const confirmDialog = await waitForMoveDialog();
    findButton(confirmDialog, 'Move profile').click();
    flushSync();
    expect(moveProfile).toHaveBeenCalledOnce();

    const host = component as {
      setProfileSelectionBlockReason: (reason: ProfileSelectionBlockReason | null) => void;
    };
    host.setProfileSelectionBlockReason('checking');
    flushSync();

    expect(document.body.querySelector(openPopoverSelector)).toBe(confirmDialog);
    expect(confirmDialog.textContent).toContain('C:/Games/Test/game.exe');
    expect(findButton(confirmDialog, 'Move profile').disabled).toBe(true);

    host.setProfileSelectionBlockReason(null);
    moveResult.resolve(false);
    await vi.waitFor(() => {
      expect(confirmDialog.querySelector('[role="alert"]')?.textContent).toContain(
        'Could not move the NVIDIA profile.',
      );
    });
    expect(document.body.querySelector(openPopoverSelector)).toBe(confirmDialog);
  });

  it('closes the move dialog only after the profile move succeeds', async () => {
    const moveProfile = vi.fn(() => Promise.resolve(true));
    const trigger = render({
      exe: executableContext({
        supportedCandidates: [executableCandidate('alternate.exe', 'bin/alternate.exe', null)],
      }),
      ownedBindingPath: 'C:/Games/Test/game.exe',
      onMoveProfile: moveProfile,
    });

    await openPopover(trigger);
    findOption(popoverContent(), 'alternate.exe').click();
    flushSync();

    const confirmDialog = await waitForMoveDialog();
    findButton(confirmDialog, 'Move profile').click();
    await vi.waitFor(() => {
      expect(moveProfile).toHaveBeenCalledOnce();
      expect(document.body.querySelector(openPopoverSelector)).toBeNull();
    });
  });

  function render({
    exe = executableContext(),
    lockReason = null,
    ownedBindingPath = null,
    onMoveProfile,
  }: {
    exe?: GameExecutableContext;
    lockReason?: ExecutableLockReason | null;
    ownedBindingPath?: string | null;
    onMoveProfile?: (path: string, selectAutomatically: boolean) => boolean | Promise<boolean>;
  } = {}): HTMLButtonElement {
    component = mount(GameExecutablePopoverTestHost, {
      target,
      props: {
        gameId: 'steam:123',
        exe,
        lockReason,
        ownedBindingPath,
        onMoveProfile,
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

async function waitForMoveDialog(): Promise<HTMLElement> {
  return await vi.waitFor(() => {
    const dialog =
      [...document.body.querySelectorAll<HTMLElement>('[role="dialog"]')].find((candidate) =>
        candidate.textContent.includes('Move the RenderPilot profile?'),
      ) ?? null;
    if (!dialog) {
      throw new Error('Profile move confirmation dialog did not open');
    }
    return dialog;
  });
}

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

function findOption(container: HTMLElement, text: string): HTMLLabelElement {
  const option = [...container.querySelectorAll<HTMLLabelElement>('label')].find((candidate) =>
    candidate.textContent.includes(text),
  );
  if (!option) {
    throw new Error(`Radio option containing "${text}" was not found`);
  }
  return option;
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
    changeError: null,
    refreshError: null,
    effectiveExe: 'game.exe',
    effectiveAbsolutePath: 'C:/Games/Test/game.exe',
    autoAbsolutePath: 'C:/Games/Test/game.exe',
    effectiveExeSource: 'auto',
    supportedCandidates: [],
    filteredOutCandidates: [],
    reload: vi.fn(() => Promise.resolve(true)),
    clear: vi.fn(),
    setOverride: vi.fn(() => Promise.resolve(true)),
    clearOverride: vi.fn(() => Promise.resolve(true)),
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
