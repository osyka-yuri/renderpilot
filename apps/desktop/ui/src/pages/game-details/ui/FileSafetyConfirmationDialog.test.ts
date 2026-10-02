/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import type { D3d12ExecutableMutationAction } from '@shared/model';
import { setLanguageMode, t } from '@shared/i18n';

import { createFileSafetyNotice, type FileSafetyNotice } from '../model/file-safety-notice-policy';
import FileSafetyConfirmationDialog from './FileSafetyConfirmationDialog.svelte';
import FileSafetyConfirmationDialogTestHost from './FileSafetyConfirmationDialog.test-host.svelte';

describe('FileSafetyConfirmationDialog', () => {
  let instance: ReturnType<typeof mount> | undefined;
  const target = document.createElement('div');

  beforeEach(() => {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      },
    );
  });

  afterEach(async () => {
    if (instance) {
      await unmount(instance);
      instance = undefined;
    }
    target.remove();
    document.body.replaceChildren();
    vi.unstubAllGlobals();
  });

  it('groups each prepared executable action and its integrity note', async () => {
    await setLanguageMode('en');
    const patch: D3d12ExecutableMutationAction = {
      kind: 'patch',
      executable_path: 'C:/Games/Test 0/game-0.exe',
      backup_path: 'C:/Games/Test 0/game-0.exe.bak',
      backup_exists: false,
      original_sdk_version: 606,
      current_sdk_version: 606,
      target_sdk_version: 619,
      requires_confirmation: true,
    };
    const actions: D3d12ExecutableMutationAction[] = [
      patch,
      {
        ...patch,
        kind: 'restore',
        executable_path: 'C:/Games/Test 1/game-1.exe',
        backup_path: 'C:/Games/Test 1/game-1.exe.bak',
        backup_exists: true,
        current_sdk_version: 619,
        target_sdk_version: 606,
      },
    ];

    instance = mount(FileSafetyConfirmationDialog, {
      target,
      props: {
        notice: null,
        actions,
        onCancel: vi.fn(),
        onConfirm: vi.fn(),
      },
    });
    flushSync();

    const dialog = document.body.querySelector<HTMLElement>('[role="dialog"]');
    if (!dialog) {
      throw new Error('Expected the executable confirmation dialog');
    }
    expect(dialog.querySelector('[data-slot="dialog-title"]')?.textContent).toBe(
      t('gameDetails.d3d12.confirm.title'),
    );
    expect(
      [...dialog.querySelectorAll<HTMLButtonElement>('button')].map((button) =>
        button.textContent.trim(),
      ),
    ).toContain(t('gameDetails.fileSafety.confirmExeMixedAction'));
    expect(dialog.textContent.split(t('gameDetails.fileSafety.generic'))).toHaveLength(2);
    expect(dialog.textContent).not.toContain('D3D12SDKVersion');
    expect(
      dialog.textContent.split(t('gameDetails.d3d12.action.planPatch', { from: 606, to: 619 })),
    ).toHaveLength(2);
    expect(
      dialog.textContent.split(t('gameDetails.d3d12.action.planRestore', { from: 619, to: 606 })),
    ).toHaveLength(2);
    expect(dialog.textContent).toContain('C:/Games/Test 0/game-0.exe');
    expect(dialog.textContent).toContain('C:/Games/Test 1/game-1.exe');
    expect(dialog.textContent).toContain(t('gameDetails.d3d12.confirm.signatureWarning'));

    const actionList = dialog.querySelector('ul');
    const scrollRegion = dialog.querySelector('[role="region"]');
    expect(actionList?.querySelectorAll('li')).toHaveLength(actions.length);
    const firstAction = actionList?.querySelector('li');
    expect(firstAction?.querySelector('[data-slot="item-title"]')?.textContent).toBe(
      t('gameDetails.d3d12.action.planPatch', { from: 606, to: 619 }),
    );
    expect(firstAction?.querySelector('code')?.textContent).toBe(patch.executable_path);
    expect(dialog.querySelector('[role="note"]')?.textContent).toContain(
      t('gameDetails.d3d12.confirm.signatureWarning'),
    );
    expect(scrollRegion?.getAttribute('tabindex')).toBe('0');
    expect(scrollRegion?.contains(actionList)).toBe(true);
    expect(dialog.querySelectorAll('[role="region"]')).toHaveLength(1);
  });

  it('labels a prepared restore as a restore action', async () => {
    await setLanguageMode('en');
    const action: D3d12ExecutableMutationAction = {
      kind: 'restore',
      executable_path: 'C:/Game/game.exe',
      backup_path: 'C:/Game/game.exe.bak',
      backup_exists: true,
      original_sdk_version: 606,
      current_sdk_version: 619,
      target_sdk_version: 606,
      requires_confirmation: true,
    };

    instance = mount(FileSafetyConfirmationDialog, {
      target,
      props: {
        notice: null,
        actions: [action],
        onCancel: vi.fn(),
        onConfirm: vi.fn(),
      },
    });
    flushSync();

    const dialog = document.body.querySelector<HTMLElement>('[role="dialog"]');
    if (!dialog) {
      throw new Error('Expected the executable restoration confirmation');
    }
    expect(
      [...dialog.querySelectorAll<HTMLButtonElement>('button')].map((button) =>
        button.textContent.trim(),
      ),
    ).toContain(t('gameDetails.fileSafety.confirmExeRestoreAction'));
    expect(dialog.textContent.split(t('gameDetails.fileSafety.generic'))).toHaveLength(2);
    expect(dialog.textContent).toContain(
      t('gameDetails.d3d12.action.planRestore', { from: 619, to: 606 }),
    );
    expect(dialog.textContent).toContain(
      t('gameDetails.d3d12.confirm.backupExists', { path: 'C:/Game/game.exe.bak' }),
    );
    expect(dialog.querySelector('li [data-slot="item-title"]')?.textContent).toBe(
      t('gameDetails.d3d12.action.planRestore', { from: 619, to: 606 }),
    );
    expect(dialog.querySelector('li code')?.textContent).toBe('C:/Game/game.exe');
    expect(dialog.textContent).not.toContain(t('gameDetails.d3d12.confirm.signatureWarning'));
    expect(dialog.querySelector('[role="note"]')).toBeNull();
  });

  it('shows the general opt-out for limited scans without exposing scan limitations', async () => {
    await setLanguageMode('en');
    const generalNotice = createFileSafetyNotice({
      game_id: 'steam:123',
      context_token: 'context-token',
      detected_engines: [],
      scan_completeness: 'limited',
    });
    const onConfirm = vi.fn();

    instance = mount(FileSafetyConfirmationDialog, {
      target,
      props: { notice: generalNotice, actions: [], onCancel: vi.fn(), onConfirm },
    });
    flushSync();

    const generalDialog = document.body.querySelector<HTMLElement>('[role="dialog"]');
    if (!generalDialog) {
      throw new Error('Expected the general file safety dialog');
    }
    expect(generalDialog.textContent).toContain(
      t('gameDetails.fileSafety.skipGeneralRiskConfirmation'),
    );
    expect(generalDialog.querySelector('[data-slot="checkbox"]')).not.toBeNull();
    expect(generalDialog.textContent).not.toContain(t('gameDetails.fileSafety.limitedDetail'));

    const checkbox = generalDialog.querySelector<HTMLButtonElement>('[data-slot="checkbox"]');
    checkbox?.click();
    flushSync();
    [...generalDialog.querySelectorAll<HTMLButtonElement>('button')]
      .find((button) =>
        button.textContent.includes(t('gameDetails.fileSafety.confirmChangeAction')),
      )
      ?.click();
    expect(onConfirm).toHaveBeenCalledWith(true);

    await unmount(instance);
    instance = undefined;
    const detectedNotice = createFileSafetyNotice({
      game_id: 'steam:123',
      context_token: 'context-token-2',
      detected_engines: ['EasyAntiCheat'],
      scan_completeness: 'limited',
    });
    instance = mount(FileSafetyConfirmationDialog, {
      target,
      props: { notice: detectedNotice, actions: [], onCancel: vi.fn(), onConfirm: vi.fn() },
    });
    flushSync();

    const detectedDialog = document.body.querySelector<HTMLElement>('[role="dialog"]');
    if (!detectedDialog) {
      throw new Error('Expected the detected-engine warning dialog');
    }
    expect(detectedDialog.textContent).toContain('Easy Anti-Cheat');
    expect(detectedDialog.querySelector('[data-slot="checkbox"]')).toBeNull();
    expect(detectedDialog.textContent).not.toContain(t('gameDetails.fileSafety.limitedDetail'));
  });

  it('resets the opt-out checkbox for every new notice object', async () => {
    await setLanguageMode('en');
    const assessment = {
      game_id: 'steam:123',
      context_token: 'repeat-context',
      detected_engines: [] as string[],
      scan_completeness: 'limited' as const,
    };
    const generalNotice = () => createFileSafetyNotice(assessment);
    const onCancel = vi.fn();
    const host: { setNotice: (notice: FileSafetyNotice | null) => void } = mount(
      FileSafetyConfirmationDialogTestHost,
      {
        target,
        props: { initialNotice: generalNotice(), onCancel, onConfirm: vi.fn() },
      },
    );
    instance = host;
    flushSync();

    function checkbox(): HTMLButtonElement {
      const element = document.body.querySelector<HTMLButtonElement>('[data-slot="checkbox"]');
      if (!element) {
        throw new Error('Expected the general warning opt-out checkbox');
      }
      return element;
    }

    expect(checkbox().getAttribute('aria-checked')).toBe('false');
    checkbox().click();
    flushSync();
    expect(checkbox().getAttribute('aria-checked')).toBe('true');
    [...document.body.querySelectorAll<HTMLButtonElement>('button')]
      .find((button) => button.textContent.includes(t('common.cancel')))
      ?.click();
    expect(onCancel).toHaveBeenCalledOnce();

    host.setNotice(null);
    flushSync();
    expect(document.body.querySelector('[role="dialog"]')?.getAttribute('data-state')).toBe(
      'closed',
    );

    host.setNotice(generalNotice());
    flushSync();
    expect(checkbox().getAttribute('aria-checked')).toBe('false');
    checkbox().click();
    flushSync();

    host.setNotice(generalNotice());
    flushSync();
    expect(checkbox().getAttribute('aria-checked')).toBe('false');
    checkbox().click();
    flushSync();

    host.setNotice(null);
    flushSync();
    expect(document.body.querySelector('[role="dialog"]')?.getAttribute('data-state')).toBe(
      'closed',
    );

    host.setNotice(generalNotice());
    flushSync();
    expect(checkbox().getAttribute('aria-checked')).toBe('false');

    host.setNotice(
      createFileSafetyNotice({
        ...assessment,
        context_token: 'detected-context',
        detected_engines: ['BattlEye'],
      }),
    );
    flushSync();
    expect(document.body.querySelector('[data-slot="checkbox"]')).toBeNull();

    host.setNotice(generalNotice());
    flushSync();
    expect(checkbox().getAttribute('aria-checked')).toBe('false');
  });
});
