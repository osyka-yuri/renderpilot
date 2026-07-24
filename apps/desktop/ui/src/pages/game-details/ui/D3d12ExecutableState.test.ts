/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import type { GameGraphicsComponent } from '@entities/game';
import { setLanguageMode } from '@shared/i18n';
import type { D3d12ExecutableAction } from '@shared/model';

import { candidate, group } from '../model/candidate-group-fixtures';
import type { GameExecutableContext } from '../model/create-game-executable-context.svelte';
import ComponentVersionRowTestHost from './ComponentVersionRow.test-host.svelte';
import GameExecutablePopover from './GameExecutablePopover.svelte';

describe('D3D12 executable state UI', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(() => {
    setLanguageMode('en');
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

  it('renders a visible fail-closed repair callout with both managed paths', () => {
    component = mount(ComponentVersionRowTestHost, {
      target,
      props: {
        component: repairComponent(),
        group: null,
        busy: false,
        onSwap: vi.fn(),
        onRollback: vi.fn(),
      },
    });
    flushSync();

    const callout = target.querySelector<HTMLElement>('[role="alert"]');
    expect(callout).not.toBeNull();
    expect(callout?.textContent).toContain('Repair required');
    expect(callout?.textContent).toContain(
      'Verify the game files in the launcher, then scan the game again.',
    );
    expect(callout?.textContent).toContain('C:/Games/Test/game.exe');
    expect(callout?.textContent).toContain('C:/Games/Test/game.exe.bak');
  });

  it('disables executable selection and explains the rollback lock', () => {
    component = mount(GameExecutablePopover, {
      target,
      props: {
        gameId: 'steam:123',
        exe: executableContext(),
        locked: true,
      },
    });
    flushSync();

    const trigger = target.querySelector<HTMLButtonElement>('button');
    expect(trigger?.disabled).toBe(true);
    expect(trigger?.getAttribute('aria-label')).toBe(
      'Executable selection is locked until the D3D12 component is fully rolled back.',
    );
  });

  it('starts a managed rollback immediately without an executable confirmation', () => {
    const onRollback = vi.fn();
    component = mount(ComponentVersionRowTestHost, {
      target,
      props: {
        component: patchedComponent(),
        group: null,
        busy: false,
        onSwap: vi.fn(),
        onRollback,
      },
    });
    flushSync();

    const rollback = target.querySelector<HTMLButtonElement>(
      'button[aria-label="Restore original D3D12Core.dll"]',
    );
    expect(rollback).not.toBeNull();

    rollback?.click();
    flushSync();

    expect(onRollback).toHaveBeenCalledOnce();
    expect(onRollback).toHaveBeenCalledWith('component:d3d12');
    expect(target.querySelector('[role="dialog"]')).toBeNull();
  });

  it('groups versions by whether they require an executable change', async () => {
    const onSwap = vi.fn();
    component = mount(ComponentVersionRowTestHost, {
      target,
      props: {
        component: patchedComponent(),
        group: group('component:d3d12', 'd3d12_agility', '1.619.4', [
          candidate('1.619.3', {
            artifact_id: 'compatible',
            d3d12_executable_action: executableAction('none', 619),
          }),
          candidate('1.618.5', {
            artifact_id: 'repatch',
            d3d12_executable_action: executableAction('patch', 618),
          }),
        ]),
        busy: false,
        onSwap,
        onRollback: vi.fn(),
      },
    });
    flushSync();

    const trigger = target.querySelector<HTMLButtonElement>('[data-slot="select-trigger"]');
    trigger?.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 }));
    trigger?.click();
    flushSync();

    await vi.waitFor(() => {
      expect(document.body.textContent).toContain('Compatible with the current EXE');
      expect(document.body.textContent).toContain('Requires an EXE change');
    });
    expect(document.body.textContent).not.toContain('The EXE will not change');
    expect(document.body.textContent).not.toContain('The patch will be updated');
    expect(document.body.querySelectorAll('[data-slot="select-separator"]').length).toBeGreaterThan(
      1,
    );

    const repatch = [
      ...document.body.querySelectorAll<HTMLElement>('[data-slot="select-item"]'),
    ].find((item) => item.textContent.includes('1.618.5'));
    repatch?.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, button: 0 }));
    flushSync();

    await vi.waitFor(() => {
      expect(onSwap).toHaveBeenCalledWith({
        componentId: 'component:d3d12',
        artifactId: 'repatch',
        isDownloaded: true,
        confirmationToken: undefined,
      });
    });
    expect(document.body.querySelector('[role="dialog"]')).toBeNull();
  });
});

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

function repairComponent(): GameGraphicsComponent {
  return {
    id: 'component:d3d12',
    game_id: 'steam:123',
    kind: 'native_library',
    technology: 'd3d12_agility',
    swappability: 'swappable',
    files: [{ path: 'C:/Games/Test/D3D12Core.dll', version: '1.619.1' }],
    rollback_available: false,
    d3d12_executable_status: {
      status: 'repair_required',
      selection_locked: true,
      executable_path: 'C:/Games/Test/game.exe',
      backup_path: 'C:/Games/Test/game.exe.bak',
      original_sdk_version: 606,
      current_sdk_version: 619,
    },
  };
}

function patchedComponent(): GameGraphicsComponent {
  return {
    ...repairComponent(),
    rollback_available: true,
    d3d12_executable_status: {
      status: 'patched',
      selection_locked: true,
      executable_path: 'C:/Games/Test/game.exe',
      backup_path: 'C:/Games/Test/game.exe.bak',
      original_sdk_version: 606,
      current_sdk_version: 619,
    },
  };
}

function executableAction(
  kind: D3d12ExecutableAction['kind'],
  targetSdk: number,
): D3d12ExecutableAction {
  return {
    kind,
    executable_path: 'C:/Games/Test/game.exe',
    backup_path: 'C:/Games/Test/game.exe.bak',
    backup_exists: true,
    original_sdk_version: 606,
    current_sdk_version: 619,
    target_sdk_version: targetSdk,
    requires_confirmation: kind === 'restore',
  };
}

function executableContext(): GameExecutableContext {
  return {
    busy: false,
    loadError: null,
    effectiveExe: 'game.exe',
    effectiveExeSource: 'auto',
    supportedCandidates: [],
    filteredOutCandidates: [],
    reload: vi.fn(),
    clear: vi.fn(),
    setOverride: vi.fn(),
    clearOverride: vi.fn(),
  } as unknown as GameExecutableContext;
}
