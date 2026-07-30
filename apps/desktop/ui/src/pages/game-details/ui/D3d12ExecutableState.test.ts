/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import type { GameLibraryComponent } from '@entities/game';
import type { SwapPlan, SwapPlanBlocker } from '@entities/operation';
import type { D3d12ExecutableAction } from '@shared/model';

import { candidate, group } from '../model/candidate-group-fixtures';
import ComponentVersionRowTestHost from './ComponentVersionRow.test-host.svelte';

const planSwap = vi.hoisted(() => vi.fn());

vi.mock('@entities/operation', () => ({ planSwap }));

describe('D3D12 executable state UI', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(() => {
    planSwap.mockReset();
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

  it('does not present a planned backup path as an existing backup', () => {
    component = mount(ComponentVersionRowTestHost, {
      target,
      props: {
        component: repairComponent(false),
        group: null,
        busy: false,
        onSwap: vi.fn(),
        onRollback: vi.fn(),
      },
    });
    flushSync();

    const callout = target.querySelector<HTMLElement>('[role="alert"]');
    expect(callout?.textContent).toContain('C:/Games/Test/game.exe');
    expect(callout?.textContent).not.toContain('C:/Games/Test/game.exe.bak');
  });

  it('does not repeat managed selection guidance in the D3D12 row', () => {
    component = mount(ComponentVersionRowTestHost, {
      target,
      props: {
        component: patchedComponent(),
        group: null,
        busy: false,
        onSwap: vi.fn(),
        onRollback: vi.fn(),
      },
    });
    flushSync();

    expect(target.textContent).toContain('EXE patched: 606 → 619');
    expect(target.textContent).not.toContain('Executable selection is locked');
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
    const repatchAction = executableAction('patch', 618);
    planSwap.mockResolvedValue(previewSwapPlan('repatch', repatchAction));
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
      expect(planSwap).toHaveBeenCalledWith('steam:123', 'component:d3d12', 'repatch');
      expect(onSwap).toHaveBeenCalledWith({
        componentId: 'component:d3d12',
        artifactId: 'repatch',
        isDownloaded: true,
        confirmationToken: undefined,
      });
    });
    expect(document.body.querySelector('[role="dialog"]')).toBeNull();
  });

  it.each([
    ['without an executable action', null],
    ['with executable action "none"', executableAction('none', 619)],
  ] as const)('preflights a Preview candidate %s', async (_case, action) => {
    const artifactId = action === null ? 'preview-no-action' : 'preview-none-action';
    planSwap.mockResolvedValue(previewSwapPlan(artifactId, action));
    const onSwap = vi.fn();
    component = mount(ComponentVersionRowTestHost, {
      target,
      props: {
        component: patchedComponent(),
        group: group('component:d3d12', 'd3d12_agility', '1.619.1', [
          {
            ...previewCandidate(artifactId),
            d3d12_executable_action: action,
          },
        ]),
        busy: false,
        onSwap,
        onRollback: vi.fn(),
      },
    });
    flushSync();

    await selectVersion('1.721.1');

    await vi.waitFor(() => {
      expect(planSwap).toHaveBeenCalledWith('steam:123', 'component:d3d12', artifactId);
      expect(onSwap).toHaveBeenCalledWith({
        componentId: 'component:d3d12',
        artifactId,
        isDownloaded: true,
        confirmationToken: undefined,
      });
    });
  });

  it.each([
    ['disabled', 'developer_mode_required', 'Windows Developer Mode is off', 'Check status'],
    [
      'unavailable',
      'developer_mode_check_unavailable',
      'Could not check Developer Mode',
      'Retry check',
    ],
  ] as const)(
    'recovers a single Preview swap when the Developer Mode check is %s',
    async (_case, blocker, dialogText, retryLabel) => {
      const artifactId = `preview-${_case}`;
      planSwap
        .mockResolvedValueOnce(previewSwapPlan(artifactId, null, [blocker]))
        .mockResolvedValueOnce(previewSwapPlan(artifactId, null));
      const onSwap = vi.fn();
      component = mount(ComponentVersionRowTestHost, {
        target,
        props: {
          component: patchedComponent(),
          group: group('component:d3d12', 'd3d12_agility', '1.619.1', [
            previewCandidate(artifactId),
          ]),
          busy: false,
          onSwap,
          onRollback: vi.fn(),
        },
      });
      flushSync();

      await selectVersion('1.721.1');

      await vi.waitFor(() => {
        expect(document.body.textContent).toContain(dialogText);
      });
      expect(onSwap).not.toHaveBeenCalled();

      const retry = [...document.body.querySelectorAll<HTMLButtonElement>('button')].find(
        (button) => button.textContent.trim() === retryLabel,
      );
      expect(retry).toBeDefined();
      retry?.click();

      await vi.waitFor(() => {
        expect(planSwap).toHaveBeenCalledTimes(2);
        expect(onSwap).toHaveBeenCalledWith({
          componentId: 'component:d3d12',
          artifactId,
          isDownloaded: true,
          confirmationToken: undefined,
        });
      });
      expect(document.body.querySelector('[role="dialog"]')).toBeNull();
    },
  );
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

function repairComponent(backupExists = true): GameLibraryComponent {
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
      backup_exists: backupExists,
      original_sdk_version: 606,
      current_sdk_version: 619,
    },
  };
}

function patchedComponent(): GameLibraryComponent {
  return {
    ...repairComponent(),
    rollback_available: true,
    d3d12_executable_status: {
      status: 'patched',
      selection_locked: true,
      executable_path: 'C:/Games/Test/game.exe',
      backup_path: 'C:/Games/Test/game.exe.bak',
      backup_exists: true,
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

function previewCandidate(artifactId: string) {
  return candidate('1.721.1', {
    artifact_id: artifactId,
    catalog_package: {
      package_id: 'microsoft.direct3d.d3d12-preview',
      release: { version: '1.721.1', channel: 'preview', label: null },
      availability: 'available',
      automatic_selection_allowed: false,
    },
    d3d12_executable_action: null,
  });
}

function previewSwapPlan(
  artifactId: string,
  action: D3d12ExecutableAction | null,
  blockers: SwapPlanBlocker[] = [],
): SwapPlan {
  return {
    operation_id: 'operation:preview',
    confirmation_token: '',
    game_id: 'steam:123',
    component_id: 'component:d3d12',
    operation_type: 'swap',
    artifact_id: artifactId,
    target_path: 'C:/Games/Test/D3D12Core.dll',
    replacement_path: 'C:/Library/D3D12Core.dll',
    original_version: '1.619.1',
    replacement_version: '1.721.1',
    original_sha256: null,
    replacement_sha256: null,
    risk_level: blockers.length > 0 ? 'blocked' : 'low',
    requires_elevation: false,
    blockers,
    warnings: [],
    files: [],
    d3d12_executable_action: action,
  };
}

async function selectVersion(version: string): Promise<void> {
  const trigger = document.body.querySelector<HTMLButtonElement>('[data-slot="select-trigger"]');
  trigger?.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 }));
  trigger?.click();
  flushSync();

  const item = await vi.waitFor(() => {
    const match = [
      ...document.body.querySelectorAll<HTMLElement>('[data-slot="select-item"]'),
    ].find((entry) => entry.textContent.includes(version));
    expect(match).toBeDefined();
    return match;
  });
  item?.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, button: 0 }));
  flushSync();
}
