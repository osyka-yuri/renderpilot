/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import type { D3d12ExecutableMutationAction } from '@shared/model';

import D3d12ExecutableConfirmDialogTestHost from './D3d12ExecutableConfirmDialog.test-host.svelte';

describe('D3d12ExecutableConfirmDialog', () => {
  let target: HTMLDivElement;
  let component: { close: () => void } | undefined;

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
      component.close();
      flushSync();
      await settleOverlays();
      await unmount(component);
      component = undefined;
    }
    vi.unstubAllGlobals();
    document.body.replaceChildren();
  });

  it('explains a first patch, the backup that will be created, and the integrity impact', async () => {
    const onConfirm = vi.fn();
    component = mount(D3d12ExecutableConfirmDialogTestHost, {
      target,
      props: {
        busy: false,
        actions: [patchAction()],
        onConfirm,
      },
    });
    flushSync();

    await vi.waitFor(() => {
      expect(document.body.textContent).toContain('C:/Games/Test/game.exe');
      expect(document.body.textContent).toContain('A patch will be applied: SDK 606 → 619');
      expect(document.body.textContent).toContain(
        'Before the change, a backup of the original EXE will be created at: C:/Games/Test/game.exe.bak',
      );
      expect(document.body.textContent).toContain(
        "After the change, the EXE's digital signature may be considered invalid and integrity checks may report that the file was modified.",
      );
      expect(document.body.textContent).toContain(
        'When you fully roll back D3D12, RenderPilot will restore the original EXE.',
      );
    });
    expect(document.body.querySelector('[data-slot="alert"][role="alert"]')).not.toBeNull();

    const confirm = [...document.body.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent.trim() === 'Change',
    );
    expect(confirm).toBeDefined();
    confirm?.click();
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it('explains an existing immutable backup without showing a patch-integrity warning', async () => {
    component = mount(D3d12ExecutableConfirmDialogTestHost, {
      target,
      props: {
        busy: false,
        actions: [
          patchAction({
            kind: 'restore',
            backup_exists: true,
            current_sdk_version: 619,
            target_sdk_version: 606,
          }),
        ],
        onConfirm: vi.fn(),
      },
    });
    flushSync();

    await vi.waitFor(() => {
      expect(document.body.textContent).toContain(
        'The original EXE will be restored: SDK 619 → 606',
      );
      expect(document.body.textContent).toContain(
        'The original EXE is already saved at: C:/Games/Test/game.exe.bak. This copy will not be overwritten.',
      );
    });
    expect(document.body.querySelector('[role="alert"]')).toBeNull();
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

function patchAction(
  overrides: Partial<D3d12ExecutableMutationAction> = {},
): D3d12ExecutableMutationAction {
  return {
    kind: 'patch',
    executable_path: 'C:/Games/Test/game.exe',
    backup_path: 'C:/Games/Test/game.exe.bak',
    backup_exists: false,
    original_sdk_version: 606,
    current_sdk_version: 606,
    target_sdk_version: 619,
    requires_confirmation: true,
    ...overrides,
  };
}
