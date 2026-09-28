/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { defaultHostFacts } from '@entities/addon';
import { createGameDetails } from '@entities/game';
import type { SettingStateResponse } from '@features/nvapi-settings';
import { setLanguageMode, t } from '@shared/i18n';
import { DesktopCommandError } from '@shared/errors';
import { registerPreviewInvoker, type DesktopInvoker } from '@shared/api-preview';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';

import GameDetailsPageTestHost from './GameDetailsPage.test-host.svelte';

const VULKAN_NOT_INSTALLED = {
  layer_detection: 'not_installed',
  layer_facts: {
    manifest_path: null,
    dll_path: null,
    version: null,
    architecture: 'unknown',
    loader_visibility: 'normal',
  },
  diagnostic_reasons: [],
  actions: {},
};

const D3D12_ACTION = {
  kind: 'patch',
  executable_path: '/games/test/Game.exe',
  backup_path: '/games/test/Game.exe.rp-backup',
  backup_exists: false,
  original_sdk_version: 606,
  current_sdk_version: 606,
  target_sdk_version: 619,
  requires_confirmation: true,
} as const;
const D3D12_REPATCH_ACTION = { ...D3D12_ACTION, requires_confirmation: false } as const;

let nvapiProfileStatusForTest: Record<string, unknown>;
let safetyEnginesForGame: string[];
let safetyCompletenessForGame: 'complete' | 'limited';
let catalogSettingWrites: { key: string; value: string }[];
let planSwapRepliesForTest: unknown[];
let enableAddonUpdateScenarioForTest: boolean;
let lumaUpdateForTest: Promise<unknown> | null;
let lumaUpdateErrorForTest: Error | null;
let safetyAssessmentForTest: (() => Promise<unknown>) | null;
let renodxAvailabilityForTest: unknown;
let renodxCheckUpdateForTest: unknown;

function unsupportedRenoDxAvailability(): Record<string, unknown> {
  return {
    state: { status: 'not_installed' },
    host_detection: 'absent',
    host_facts: defaultHostFacts('stable'),
    actions: {},
    reshade_stable_supported: true,
    renodx_addon: null,
    install_torn: false,
    outcome: { kind: 'unsupported' },
    manual_install: null,
    vulkan_layer: VULKAN_NOT_INSTALLED,
  };
}

function unsupportedLumaAvailability(): unknown {
  return {
    state: { status: 'not_installed' },
    host_detection: 'absent',
    host_facts: defaultHostFacts('nightly'),
    actions: {},
    min_reshade_version: '6.0.0',
    vcredist_present: null,
    vcredist_installer_url: 'https://aka.ms/vs/17/release/vc_redist.x64.exe',
    install_torn: false,
    uninstall_blocked_by: null,
    outcome: { kind: 'unsupported' },
  };
}

function unsupportedOptiScalerAvailability(): unknown {
  return {
    game_id: 'steam:123',
    install: { installed: false, release: null },
    eligibility: {
      available: false,
      block_code: 'catalog_unsupported',
    },
    selected_release: null,
    relocation: null,
    proxy_conflict: null,
    compatibility: { status: 'unsupported', declared_inputs: [], launch: null, guidance: [] },
    prerequisite: { state: 'none' },
    modules: [],
    lifecycle: {
      update_available: false,
      repair_required: false,
      drifted: false,
      unmanaged: false,
      maintenance_available: false,
      maintenance_block_code: null,
    },
  };
}

function unavailableDlssFixAvailability(): unknown {
  return {
    kind: 'binding',
    state: 'none',
    actions: [],
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function deleteProfileButton(target: HTMLElement): HTMLButtonElement | undefined {
  return [...target.querySelectorAll<HTMLButtonElement>('button')].find((button) =>
    button.textContent.includes('Delete profile'),
  );
}

function detailsWithDlss() {
  return createGameDetails({
    components: [
      {
        id: 'component:test-dlss',
        game_id: 'game:test',
        kind: 'library',
        technology: 'dlss_super_resolution',
        swappability: 'swappable',
        files: [],
        rollback_available: false,
        d3d12_executable_status: null,
      },
    ],
  });
}

function driverSetting(overrides: Partial<SettingStateResponse> = {}): SettingStateResponse {
  return {
    setting_key: 'dlss_sr_render_preset',
    setting_label: 'DLSS preset',
    value_type: 'dword',
    dll_kind: 'sr',
    family: 'sr',
    category: 'DLSS Super Resolution',
    description: null,
    min_driver: null,
    current: { wire: 'default', label: 'Default', dword: 0 },
    current_is_explicit: false,
    predefined: null,
    original: null,
    is_current_predefined: true,
    effective_exe: 'C:/Games/Test/game.exe',
    effective_exe_source: 'auto',
    has_profile_for_exe: true,
    nvapi_available: true,
    catalog_readiness: 'ready',
    available_values: [],
    dll_info: null,
    warnings: [],
    ...overrides,
  };
}

function d3dUpdateDetails(installPath = '/games/test') {
  return createGameDetails({
    game: {
      identity: { id: 'steam:123', title: 'Test Game', launcher: 'Steam' },
      install_path: installPath,
    },
    components: [
      {
        id: 'component-d3d12',
        game_id: 'steam:123',
        kind: 'library',
        technology: 'd3d12_agility',
        swappability: 'swappable',
        files: [],
        rollback_available: false,
        d3d12_executable_status: null,
      },
    ],
    candidate_groups: [
      {
        component_id: 'component-d3d12',
        technology: 'd3d12_agility',
        file_path: 'Game.dll',
        version_report: {
          kind: 'known',
          technical_version: '1.0',
          release_label: null,
          catalog_release: null,
        },
        automatic_candidate_artifact_id: 'artifact-d3d12',
        candidates: [
          {
            artifact_id: 'artifact-d3d12',
            file_name: 'Game.dll',
            file_path: null,
            technical_version: '2.0',
            release_label: 'Latest',
            source_game_id: null,
            comparison: 'newer',
            catalog_package: null,
            is_downloaded: true,
            is_debug: false,
            sha256: 'sha256-d3d12',
            d3d12_executable_action: D3D12_ACTION,
          },
        ],
      },
    ],
  });
}

function addonUpdateDetails(installPath: string) {
  return createGameDetails({
    game: {
      identity: { id: 'steam:123', title: 'Test Game', launcher: 'Steam' },
      install_path: installPath,
    },
    addon_capabilities: ['luma', 'optiscaler'],
  });
}

function streamlineUpdateDetails(optionId: string) {
  return createGameDetails({
    game: {
      identity: { id: 'steam:123', title: 'Test Game', launcher: 'Steam' },
      install_path: '/games/test',
    },
    components: [
      {
        id: 'component:streamline',
        game_id: 'steam:123',
        kind: 'native_library',
        technology: 'nvidia_streamline',
        swappability: 'bundle_only',
        files: [],
        rollback_available: false,
        d3d12_executable_status: null,
      },
    ],
    candidate_groups: [
      {
        component_id: 'component:streamline',
        technology: 'nvidia_streamline',
        file_path: 'streamline.dll',
        version_report: {
          kind: 'known',
          technical_version: '2.3.0',
          release_label: null,
          catalog_release: null,
        },
        automatic_candidate_artifact_id: null,
        candidates: [
          {
            artifact_id: 'streamline-2.4.0',
            file_name: 'sl.interposer.dll',
            file_path: null,
            technical_version: '2.4.0',
            release_label: null,
            source_game_id: null,
            comparison: 'newer_version',
            catalog_package: null,
            is_downloaded: true,
            is_debug: false,
            sha256: 'streamline-hash',
            d3d12_executable_action: null,
          },
        ],
      },
    ],
    streamline_candidate_options: [
      {
        option_id: optionId,
        release: { version: '2.4.0', channel: 'stable', label: null },
        items: [{ component_id: 'component:streamline', artifact_id: 'streamline-2.4.0' }],
      },
    ],
  });
}

async function dialogContaining(text: string): Promise<HTMLElement> {
  return vi.waitFor(() => {
    const dialog = [...document.body.querySelectorAll<HTMLElement>('[role="dialog"]')].find(
      (item) => item.textContent.includes(text),
    );
    if (!dialog) {
      throw new Error(
        `Expected a dialog containing: ${text}; dialogs: ${[...document.body.querySelectorAll<HTMLElement>('[role="dialog"]')].map((item) => item.textContent).join(' || ')}`,
      );
    }
    return dialog;
  });
}

function clickDialogButton(dialog: HTMLElement, name: string): void {
  const button = [...dialog.querySelectorAll<HTMLButtonElement>('button')].find((candidate) =>
    candidate.textContent.includes(name),
  );
  if (!button) {
    throw new Error(`Expected dialog button: ${name}`);
  }
  button.click();
}

async function chooseSelectOption(trigger: HTMLButtonElement, value: string): Promise<void> {
  trigger.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 }));
  trigger.click();
  flushSync();

  const option = await vi.waitFor(() => {
    const candidate = document.body.querySelector<HTMLElement>(
      `[role="option"][data-value="${value}"]`,
    );
    if (!candidate) {
      throw new Error(`Expected option ${value}`);
    }
    return candidate;
  });
  option.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 }));
  option.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, button: 0 }));
  option.click();
  flushSync();
}

describe('GameDetailsPage', () => {
  let target: HTMLDivElement;
  let component: object | undefined;
  let disposeInvoker: (() => void) | undefined;
  let invokedCommands: string[];
  let lumaUpdatePayloads: (Record<string, unknown> | undefined)[];
  let unexpectedCommands: string[];
  let executableOverrideGameIds: string[];
  let profileStatusForGame: (gameId: string) => Promise<unknown>;
  let executableForGame: (gameId: string) => Promise<unknown>;
  let executableCandidatesForGame: (gameId: string) => Promise<unknown>;
  let nvidiaSettingsForGame: (gameId: string) => Promise<SettingStateResponse[]>;
  let deleteProfileForGame: (gameId: string) => Promise<unknown>;

  beforeEach(async () => {
    await setLanguageMode('en');
    clearAllNotifications();
    safetyEnginesForGame = [];
    safetyCompletenessForGame = 'complete';
    catalogSettingWrites = [];
    planSwapRepliesForTest = [];
    enableAddonUpdateScenarioForTest = false;
    lumaUpdateForTest = null;
    lumaUpdateErrorForTest = null;
    safetyAssessmentForTest = null;
    renodxAvailabilityForTest = null;
    renodxCheckUpdateForTest = null;
    nvapiProfileStatusForTest = {
      selectedExecutable: null,
      bindingPath: null,
      profileName: null,
      state: 'noExecutable',
      isPredefined: null,
      ownedByThisGame: false,
      canCreate: false,
      canDelete: false,
      pendingOperation: null,
      pendingOperationGameId: null,
      detail: null,
    };
    Object.defineProperty(window, 'ResizeObserver', {
      configurable: true,
      value: class ResizeObserverMock {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      },
    });
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
    invokedCommands = [];
    lumaUpdatePayloads = [];
    unexpectedCommands = [];
    executableOverrideGameIds = [];
    profileStatusForGame = () => Promise.resolve(nvapiProfileStatusForTest);
    executableForGame = () => Promise.resolve(null);
    executableCandidatesForGame = () => Promise.resolve([]);
    nvidiaSettingsForGame = () => Promise.resolve([]);
    deleteProfileForGame = () => Promise.resolve(undefined);
    const invoker = ((command: string, payload?: Record<string, unknown>) => {
      invokedCommands.push(command);
      if (command === 'renodx_availability') {
        return Promise.resolve(renodxAvailabilityForTest ?? unsupportedRenoDxAvailability());
      }
      if (command === 'renodx_check_update') {
        return Promise.resolve(
          renodxCheckUpdateForTest ?? {
            addon: 'current',
            host: 'current',
            dlssFix: null,
            overall: 'current',
          },
        );
      }
      if (command === 'luma_availability' && enableAddonUpdateScenarioForTest) {
        return Promise.resolve({
          state: {
            status: 'installed',
            version: 'Build 515',
            addon_dated: null,
            installed_at: 1,
            updated_at: 1,
            reshade_channel: 'nightly',
            launch_args: [],
          },
          outcome: { kind: 'unsupported' },
          host_detection: 'present',
          host_facts: defaultHostFacts('nightly'),
          actions: {},
          min_reshade_version: '6.7.0',
          vcredist_present: null,
          vcredist_installer_url: 'https://aka.ms/vs/17/release/vc_redist.x64.exe',
          install_torn: false,
          uninstall_blocked_by: null,
        });
      }
      if (command === 'luma_availability') {
        return Promise.resolve(unsupportedLumaAvailability());
      }
      if (command === 'get_optiscaler_availability' && enableAddonUpdateScenarioForTest) {
        return Promise.resolve({
          game_id: 'steam:123',
          launcher: 'Steam',
          install: { installed: true, release: '1.0.0' },
          eligibility: { available: true, block_code: null },
          selected_release: 'stable',
          relocation: null,
          proxy_conflict: null,
          compatibility: {
            status: 'working',
            declared_inputs: [],
            launch: null,
            guidance: [],
          },
          prerequisite: { state: 'none' },
          modules: [],
          lifecycle: {
            update_available: true,
            repair_required: false,
            drifted: false,
            unmanaged: false,
            maintenance_available: true,
            maintenance_block_code: null,
          },
        });
      }
      if (command === 'get_optiscaler_availability') {
        return Promise.resolve(unsupportedOptiScalerAvailability());
      }
      if (command === 'renodx_dlss_fix_availability') {
        return Promise.resolve(unavailableDlssFixAvailability());
      }
      if (command === 'get_game_file_safety_assessment') {
        if (safetyAssessmentForTest) {
          return safetyAssessmentForTest();
        }
        const requestedGameId = typeof payload?.gameId === 'string' ? payload.gameId : 'steam:123';
        return Promise.resolve({
          game_id: requestedGameId,
          context_token: 'game-safety-token',
          detected_engines: safetyEnginesForGame,
          scan_completeness: safetyCompletenessForGame,
        });
      }
      if (command === 'plan_swap') {
        return Promise.resolve(
          planSwapRepliesForTest.shift() ?? {
            blockers: [],
            confirmation_token: 'd3d12-confirmation-token',
            d3d12_executable_action: D3D12_ACTION,
          },
        );
      }
      if (command === 'luma_check_update' && enableAddonUpdateScenarioForTest) {
        return Promise.resolve({
          addon: 'available',
          host: 'current',
          dgvoodoo: null,
          overall: 'available',
        });
      }
      if (command === 'luma_update' && enableAddonUpdateScenarioForTest) {
        lumaUpdatePayloads.push(payload);
        if (lumaUpdateErrorForTest !== null) {
          return Promise.reject(lumaUpdateErrorForTest);
        }
        return (
          lumaUpdateForTest ??
          Promise.resolve({
            status: 'installed',
            version: 'Build 516',
            addon_dated: null,
            installed_at: 1,
            updated_at: 2,
            reshade_channel: 'nightly',
            launch_args: [],
          })
        );
      }
      if (command === 'get_catalog_setting') {
        return Promise.resolve({ value: null });
      }
      if (command === 'set_catalog_setting') {
        catalogSettingWrites.push({
          key: typeof payload?.key === 'string' ? payload.key : '',
          value: typeof payload?.value === 'string' ? payload.value : '',
        });
        return Promise.resolve({ saved: true });
      }
      if (command === 'get_shared_vulkan_safety_assessment') {
        return Promise.resolve({ context_token: 'shared-vulkan-safety-token' });
      }
      if (command === 'resolve_game_executable') {
        return executableForGame(typeof payload?.gameId === 'string' ? payload.gameId : '');
      }
      if (command === 'list_game_executable_candidates') {
        return executableCandidatesForGame(
          typeof payload?.gameId === 'string' ? payload.gameId : '',
        );
      }
      if (command === 'get_nvapi_profile_status') {
        return profileStatusForGame(typeof payload?.gameId === 'string' ? payload.gameId : '');
      }
      if (command === 'list_nvapi_setting_states') {
        return nvidiaSettingsForGame(typeof payload?.gameId === 'string' ? payload.gameId : '');
      }
      if (command === 'delete_nvapi_profile') {
        return deleteProfileForGame(typeof payload?.gameId === 'string' ? payload.gameId : '');
      }
      if (command === 'set_game_executable_override') {
        executableOverrideGameIds.push(typeof payload?.gameId === 'string' ? payload.gameId : '');
        return Promise.resolve(undefined);
      }
      unexpectedCommands.push(command);
      return Promise.reject(new Error(`Unexpected command in GameDetailsPage test: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
      component = undefined;
    }
    expect(unexpectedCommands).toEqual([]);
    disposeInvoker?.();
    disposeInvoker = undefined;
    clearAllNotifications();
    delete (HTMLElement.prototype as Partial<HTMLElement>).hasPointerCapture;
    delete (HTMLElement.prototype as Partial<HTMLElement>).releasePointerCapture;
    target.remove();
  });

  it('shows a concise recovery status for a pending NVIDIA operation', async () => {
    const onOpenGameDetails = vi.fn();
    nvapiProfileStatusForTest = {
      ...nvapiProfileStatusForTest,
      selectedExecutable: 'C:/Games/shared/Game.exe',
      profileName: 'NVIDIA Shared Profile',
      state: 'pending',
      pendingOperation: 'setting',
      pendingOperationGameId: 'manual:game-a',
    };
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: detailsWithDlss(), onOpenGameDetails },
    });
    flushSync();

    await vi.waitFor(() => {
      expect(target.textContent).toContain('An NVIDIA profile operation needs recovery.');
      expect(target.textContent).toContain(
        'Recovery is blocked by a pending operation for another game.',
      );
    });
    expect(target.textContent).not.toContain('manual:game-a');
    expect(target.textContent).not.toContain('internal English diagnostic detail');
    [...target.querySelectorAll('button')]
      .find((button) => button.textContent.includes('View game details'))
      ?.click();
    expect(onOpenGameDetails).toHaveBeenCalledWith('manual:game-a');
  });

  it('shows the recorded executable for a profile conflict without claiming the driver changed', async () => {
    nvapiProfileStatusForTest = {
      ...nvapiProfileStatusForTest,
      selectedExecutable: 'C:/Games/Test/alternate.exe',
      bindingPath: 'C:/Games/Test/game.exe',
      profileName: 'RenderPilot - Test',
      state: 'conflict',
      ownedByThisGame: true,
      detail: 'internal English diagnostic detail',
    };
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: createGameDetails() },
    });
    flushSync();

    await vi.waitFor(() => {
      expect(target.textContent).toContain(
        'The saved NVIDIA profile state could not be verified. Profile changes are blocked.',
      );
      expect(target.textContent).toContain('C:/Games/Test/game.exe');
    });
    expect(target.textContent).not.toContain('internal English diagnostic detail');
    expect(target.textContent).not.toContain('no longer matches the driver profile');
  });

  it('keeps ordinary games without a DLSS driver-settings surface free of profile controls', async () => {
    nvapiProfileStatusForTest = {
      ...nvapiProfileStatusForTest,
      state: 'missing',
      canCreate: true,
    };
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: createGameDetails() },
    });
    flushSync();

    await vi.waitFor(() => {
      expect(invokedCommands).toContain('get_nvapi_profile_status');
    });
    expect(target.querySelector('[aria-label="NVIDIA profile"]')).toBeNull();
    expect(target.textContent).not.toContain('Create profile');
  });

  it('places profile creation inside the NVIDIA tab before the DLSS card', async () => {
    nvidiaSettingsForGame = () => Promise.resolve([driverSetting()]);
    nvapiProfileStatusForTest = {
      ...nvapiProfileStatusForTest,
      state: 'missing',
      canCreate: true,
    };
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: detailsWithDlss() },
    });
    flushSync();

    const row = await vi.waitFor(() => {
      const control = target.querySelector<HTMLElement>(
        '[role="group"][aria-label="NVIDIA profile"]',
      );
      if (!control) {
        throw new Error('Expected the NVIDIA profile control');
      }
      expect(control.textContent).toContain('Create profile');
      return control;
    });
    const panel = row.closest('[role="tabpanel"]');
    if (!panel) {
      throw new Error('Expected the NVIDIA profile control inside a tab panel');
    }
    const dlssCard = panel.querySelector('[data-slot="card"]');
    if (!dlssCard) {
      throw new Error('Expected the DLSS card in the NVIDIA tab');
    }
    expect(row.compareDocumentPosition(dlssCard) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('does not offer profile creation when a DLSS DLL has no driver setting rows', async () => {
    nvapiProfileStatusForTest = {
      ...nvapiProfileStatusForTest,
      state: 'missing',
      canCreate: true,
    };
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: detailsWithDlss() },
    });
    flushSync();

    await vi.waitFor(() => {
      expect(invokedCommands).toContain('list_nvapi_setting_states');
      expect(invokedCommands).toContain('get_nvapi_profile_status');
      expect(target.querySelector('[aria-label="NVIDIA profile"]')?.textContent).toContain(
        'No NVIDIA profile.',
      );
    });
    expect(target.textContent).not.toContain('Create profile');
    expect(target.textContent).not.toContain('override its settings');
  });

  it('does not offer profile creation when NVAPI is unavailable despite setting rows', async () => {
    nvidiaSettingsForGame = () => Promise.resolve([driverSetting({ nvapi_available: false })]);
    nvapiProfileStatusForTest = {
      ...nvapiProfileStatusForTest,
      state: 'missing',
      canCreate: true,
    };
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: detailsWithDlss() },
    });
    flushSync();

    await vi.waitFor(() => {
      expect(invokedCommands).toContain('list_nvapi_setting_states');
      expect(target.querySelector('[aria-label="NVIDIA profile"]')?.textContent).toContain(
        'No NVIDIA profile.',
      );
    });
    expect(target.textContent).not.toContain('Create profile');
    expect(target.textContent).not.toContain('override its settings');
  });

  it('returns focus to the game heading after deleting a recovery-only profile', async () => {
    let statusRequests = 0;
    profileStatusForGame = () => {
      statusRequests += 1;
      return Promise.resolve({
        ...nvapiProfileStatusForTest,
        selectedExecutable: 'C:/Games/Test/game.exe',
        bindingPath: 'C:/Games/Test/game.exe',
        profileName: 'RenderPilot - Test',
        state: statusRequests === 1 ? 'owned' : 'missing',
        ownedByThisGame: statusRequests === 1,
        canDelete: statusRequests === 1,
      });
    };
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: createGameDetails() },
    });
    flushSync();

    const deleteButton = await vi.waitFor(() => {
      const button = deleteProfileButton(target);
      expect(button).toBeDefined();
      if (!button) {
        throw new Error('Expected the recovery profile delete action');
      }
      return button;
    });
    deleteButton.click();

    const dialog = await vi.waitFor(() => {
      const content = document.body.querySelector<HTMLElement>('[data-slot="dialog-content"]');
      expect(content?.textContent).toContain('Delete RenderPilot - Test?');
      if (!content) {
        throw new Error('Expected delete confirmation');
      }
      return content;
    });
    [...dialog.querySelectorAll<HTMLButtonElement>('button')]
      .find((button) => button.textContent.includes('Delete profile'))
      ?.click();

    await vi.waitFor(() => {
      expect(target.querySelector('[aria-label="NVIDIA profile"]')).toBeNull();
      expect(document.activeElement).toBe(target.querySelector('#game-details-title'));
    });
  });

  it('keeps a no-tab lookup error recoverable without offering profile creation', async () => {
    let requests = 0;
    profileStatusForGame = () => {
      requests += 1;
      return requests === 1
        ? Promise.reject(new Error('Profile lookup failed'))
        : Promise.resolve({
            ...nvapiProfileStatusForTest,
            state: 'predefined',
            isPredefined: true,
          });
    };
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: createGameDetails() },
    });
    flushSync();

    await vi.waitFor(() => {
      expect(target.textContent).toContain('The NVIDIA profile could not be inspected.');
      expect(
        [...target.querySelectorAll('button')].some((button) =>
          button.textContent.includes('Retry'),
        ),
      ).toBe(true);
    });
    expect(target.textContent).not.toContain('Create profile');
    [...target.querySelectorAll<HTMLButtonElement>('button')]
      .find((button) => button.textContent.includes('Retry'))
      ?.click();

    await vi.waitFor(() => {
      expect(requests).toBe(2);
      expect(target.querySelector('[aria-label="NVIDIA profile"]')).toBeNull();
    });
    expect(target.textContent).not.toContain('Create profile');
  });

  it('keeps the profile status row in a Streamline-only NVIDIA tab without offering creation', async () => {
    nvapiProfileStatusForTest = {
      ...nvapiProfileStatusForTest,
      state: 'missing',
      canCreate: true,
    };
    const details = createGameDetails({
      components: [
        {
          id: 'component:test-streamline',
          game_id: 'game:test',
          kind: 'library',
          technology: 'nvidia_streamline',
          swappability: 'swappable',
          files: [],
          rollback_available: false,
          d3d12_executable_status: null,
        },
      ],
    });
    component = mount(GameDetailsPageTestHost, { target, props: { details } });
    flushSync();

    await vi.waitFor(() => {
      expect(invokedCommands).toContain('get_nvapi_profile_status');
      expect(target.querySelector('[aria-label="NVIDIA profile"]')?.textContent).toContain(
        'No NVIDIA profile.',
      );
    });
    expect(target.textContent).not.toContain('Create profile');
    expect(
      target.querySelector('[aria-label="NVIDIA profile"]')?.closest('[role="tabpanel"]'),
    ).not.toBeNull();
  });

  it('does not carry an owned profile binding across Windows game switches or onto non-Windows games', async () => {
    const initialBindingPath = 'C:/Games/game-1/game.exe';
    const secondBindingPath = 'C:/Games/game-2/game.exe';
    const deferredSecondGameStatus = deferred<unknown>();
    let secondGameStatusRequests = 0;

    const ownedStatus = (gameId: string, bindingPath: string) => ({
      selectedExecutable: bindingPath,
      bindingPath,
      profileName: `Owned ${gameId}`,
      state: 'owned',
      isPredefined: false,
      ownedByThisGame: true,
      canCreate: false,
      canDelete: true,
      pendingOperation: null,
      pendingOperationGameId: null,
      detail: null,
    });

    profileStatusForGame = (gameId) => {
      if (gameId === 'game-1') {
        return Promise.resolve(ownedStatus(gameId, initialBindingPath));
      }
      if (gameId === 'game-2') {
        secondGameStatusRequests += 1;
        return secondGameStatusRequests === 1
          ? deferredSecondGameStatus.promise
          : Promise.resolve(ownedStatus(gameId, secondBindingPath));
      }
      return Promise.resolve(nvapiProfileStatusForTest);
    };
    executableForGame = (gameId) =>
      Promise.resolve({
        file_name: `${gameId}.exe`,
        absolute_path: `C:/Games/${gameId}/${gameId}.exe`,
        auto_absolute_path: `C:/Games/${gameId}/${gameId}.exe`,
        source: 'auto',
      });
    executableCandidatesForGame = (gameId) =>
      Promise.resolve(
        ['game.exe', 'alternate.exe'].map((fileName) => ({
          relative_path: fileName,
          file_name: fileName,
          absolute_path: `C:/Games/${gameId}/${fileName}`,
          size_bytes: 1,
          depth: 0,
          rank_score: 0,
          rejection: null,
          rejection_token: null,
        })),
      );

    const detailsFor = (gameId: string, platform: string) =>
      createGameDetails({
        game: {
          identity: { id: gameId, title: gameId, launcher: 'Manual' },
          platform,
          runtime: platform === 'Windows' ? 'NativeWindows' : 'NativeLinux',
          install_path: `/test/${gameId}`,
          can_remove_from_catalog: true,
        },
      });

    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: detailsFor('game-1', 'Windows') },
    });
    flushSync();
    await vi.waitFor(() => {
      expect(deleteProfileButton(target)).toBeDefined();
      expect(
        target.querySelector('button[aria-label="Game executable: game-1.exe"]'),
      ).not.toBeNull();
    });

    const host = component as {
      replaceDetails: (details: ReturnType<typeof createGameDetails>) => void;
    };
    host.replaceDetails(detailsFor('game-2', 'Windows'));
    flushSync();
    await vi.waitFor(() => {
      expect(
        target.querySelector('button[aria-label="Game executable: game-2.exe"]'),
      ).not.toBeNull();
      expect(secondGameStatusRequests).toBe(1);
    });

    const gameTwoTrigger = target.querySelector<HTMLButtonElement>(
      'button[aria-label="Game executable: game-2.exe"]',
    );
    if (!gameTwoTrigger) {
      throw new Error('Game 2 executable selector was not rendered');
    }
    expect(gameTwoTrigger.getAttribute('aria-disabled')).toBe('true');
    gameTwoTrigger.click();
    flushSync();
    expect(document.body.querySelector('[role="dialog"]')).toBeNull();
    expect(executableOverrideGameIds).not.toContain('game-2');

    deferredSecondGameStatus.resolve(ownedStatus('game-2', secondBindingPath));
    await vi.waitFor(() => {
      expect(deleteProfileButton(target)).toBeDefined();
    });
    const resolvedGameTwoTrigger = target.querySelector<HTMLButtonElement>(
      'button[aria-label="Game executable: game-2.exe"]',
    );
    if (!resolvedGameTwoTrigger) {
      throw new Error('Game 2 executable selector was not rendered after profile verification');
    }
    expect(resolvedGameTwoTrigger.getAttribute('aria-disabled')).toBeNull();
    resolvedGameTwoTrigger.click();
    flushSync();
    await vi.waitFor(() => {
      expect(document.body.querySelector('[role="dialog"]')).not.toBeNull();
    });
    const gameTwoPopover = document.body.querySelector<HTMLElement>('[role="dialog"]');
    if (!gameTwoPopover) {
      throw new Error('Game 2 executable selector did not open');
    }
    const gameTwoAlternate = [...gameTwoPopover.querySelectorAll('label')].find((label) =>
      label.textContent.includes('alternate.exe'),
    );
    if (!gameTwoAlternate) {
      throw new Error('Game 2 alternate executable option was not rendered');
    }
    gameTwoAlternate.click();
    flushSync();
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain('Move the RenderPilot profile?');
    });
    expect(executableOverrideGameIds).not.toContain('game-2');
    expect(document.body.textContent).toContain(secondBindingPath);
    const cancelMove = [...document.body.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent.includes('Cancel'),
    );
    if (!cancelMove) {
      throw new Error('Profile move cancellation button was not rendered');
    }
    cancelMove.click();
    flushSync();
    await vi.waitFor(() => {
      expect(document.body.textContent).not.toContain('Move the RenderPilot profile?');
    });

    host.replaceDetails(detailsFor('game-3', 'Linux'));
    flushSync();
    await vi.waitFor(() => {
      expect(
        target.querySelector('button[aria-label="Game executable: game-3.exe"]'),
      ).not.toBeNull();
      expect(deleteProfileButton(target)).toBeUndefined();
    });

    const gameThreeTrigger = target.querySelector<HTMLButtonElement>(
      'button[aria-label="Game executable: game-3.exe"]',
    );
    if (!gameThreeTrigger) {
      throw new Error('Game 3 executable selector was not rendered');
    }
    gameThreeTrigger.click();
    flushSync();
    await vi.waitFor(() => {
      expect(document.body.querySelector('[role="dialog"]')).not.toBeNull();
    });
    const gameThreePopover = document.body.querySelector<HTMLElement>('[role="dialog"]');
    if (!gameThreePopover) {
      throw new Error('Game 3 executable selector did not open');
    }
    const gameThreeAlternate = [...gameThreePopover.querySelectorAll('label')].find((label) =>
      label.textContent.includes('alternate.exe'),
    );
    if (!gameThreeAlternate) {
      throw new Error('Game 3 alternate executable option was not rendered');
    }
    gameThreeAlternate.click();
    flushSync();
    await vi.waitFor(() => {
      expect(executableOverrideGameIds).toContain('game-3');
    });
    expect(target.textContent).not.toContain('Move the RenderPilot profile?');
  });

  it('prefetches operations on pointer and keyboard intent before opening them', () => {
    const onPreloadOperations = vi.fn();
    const onOpenOperations = vi.fn();

    component = mount(GameDetailsPageTestHost, {
      target,
      props: {
        details: createGameDetails(),
        onOpenOperations,
        onPreloadOperations,
      },
    });
    flushSync();

    const operations = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent.trim() === 'Operations Journal',
    );
    expect(operations).toBeDefined();

    operations?.dispatchEvent(new Event('pointerenter'));
    operations?.focus();
    operations?.click();

    expect(onPreloadOperations).toHaveBeenCalledTimes(2);
    expect(onOpenOperations).toHaveBeenCalledTimes(1);
  });

  it('keeps disabled Update All focusable and explains why it cannot run', async () => {
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: createGameDetails() },
    });
    flushSync();

    const updateAll = target.querySelector<HTMLButtonElement>(
      'button[aria-label="All stable versions are up to date"]',
    );
    expect(updateAll).toBeDefined();
    expect(updateAll?.getAttribute('aria-disabled')).toBe('true');
    expect(updateAll?.disabled).toBe(false);

    const assessmentRequests = invokedCommands.filter(
      (command) => command === 'get_game_file_safety_assessment',
    ).length;
    updateAll?.focus();
    flushSync();
    await vi.waitFor(() => {
      expect(document.body.querySelector('[role="tooltip"]')?.textContent).toContain(
        'All stable versions are up to date',
      );
    });
    updateAll?.click();
    await Promise.resolve();
    expect(
      invokedCommands.filter((command) => command === 'get_game_file_safety_assessment'),
    ).toHaveLength(assessmentRequests);
  });

  it('captures one risk approval and reuses its token for the complete Update All batch', async () => {
    enableAddonUpdateScenarioForTest = true;
    planSwapRepliesForTest = [
      {
        blockers: [],
        confirmation_token: 'first-patch-token',
        d3d12_executable_action: D3D12_ACTION,
      },
      {
        blockers: [],
        confirmation_token: '',
        d3d12_executable_action: D3D12_REPATCH_ACTION,
      },
    ];
    const onBulkSwap = vi.fn(() => Promise.resolve());
    const details = createGameDetails({
      game: { identity: { id: 'steam:123', title: 'Test Game', launcher: 'Steam' } },
      addon_capabilities: ['luma'],
      components: [
        {
          id: 'component-1',
          game_id: 'steam:123',
          kind: 'library',
          technology: 'd3d12_agility',
          swappability: 'swappable',
          files: [],
          rollback_available: false,
          d3d12_executable_status: null,
        },
      ],
      candidate_groups: [
        {
          component_id: 'component-1',
          technology: 'd3d12_agility',
          file_path: 'game.dll',
          version_report: { kind: 'unknown' },
          automatic_candidate_artifact_id: 'artifact-1',
          candidates: [
            {
              artifact_id: 'artifact-1',
              file_name: 'game.dll',
              file_path: null,
              technical_version: '2',
              release_label: 'Latest',
              source_game_id: null,
              comparison: 'newer',
              catalog_package: null,
              is_downloaded: true,
              is_debug: false,
              sha256: 'sha256',
              d3d12_executable_action: D3D12_ACTION,
            },
          ],
        },
      ],
    });
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details, onBulkSwap },
    });
    flushSync();
    expect(invokedCommands).not.toContain('get_game_file_safety_assessment');

    const updateAll = await vi.waitFor(() => {
      const button = target.querySelector<HTMLButtonElement>('button[aria-label="Update all (2)"]');
      if (!button) {
        throw new Error('Expected the library and Luma updates to be available');
      }
      return button;
    });
    updateAll.click();
    flushSync();

    const dialog = await vi.waitFor(() => {
      const candidate = [...document.body.querySelectorAll<HTMLElement>('[role="dialog"]')].find(
        (item) => item.textContent.includes(t('gameDetails.fileSafety.generic')),
      );
      if (!candidate) {
        throw new Error('Expected the game-file risk confirmation');
      }
      return candidate;
    });
    expect(
      dialog.textContent.match(
        /Changes to multiplayer game files may result in account restrictions or a ban\./g,
      ),
    ).toHaveLength(1);
    expect(dialog.textContent).toContain('/games/test/Game.exe.rp-backup');
    expect(dialog.textContent).toContain(t('gameDetails.fileSafety.skipGeneralRiskConfirmation'));
    expect(dialog.querySelector('[data-slot="dialog-title"]')?.textContent).toBe(
      t('gameDetails.d3d12.confirm.title'),
    );
    expect(dialog.textContent).toContain(t('gameDetails.updateAll.action'));
    dialog.querySelector<HTMLButtonElement>('[data-slot="checkbox"]')?.click();
    clickDialogButton(dialog, t('gameDetails.updateAll.action'));

    await vi.waitFor(() => {
      expect(onBulkSwap).toHaveBeenCalledOnce();
      expect(lumaUpdatePayloads).toHaveLength(1);
    });
    expect(onBulkSwap).toHaveBeenCalledWith([
      expect.objectContaining({
        componentId: 'component-1',
        artifactId: 'artifact-1',
        confirmationToken: 'first-patch-token',
        gameContextToken: 'game-safety-token',
      }),
    ]);
    expect(lumaUpdatePayloads[0]).toEqual(
      expect.objectContaining({ gameId: 'steam:123', gameContextToken: 'game-safety-token' }),
    );
    await vi.waitFor(() => {
      expect(catalogSettingWrites).toContainEqual({
        key: 'game_file_safety_warning_v1',
        value: 'true',
      });
    });
    expect(
      invokedCommands.filter((command) => command === 'get_game_file_safety_assessment'),
    ).toHaveLength(1);
    expect(invokedCommands.indexOf('plan_swap')).toBeLessThan(
      invokedCommands.lastIndexOf('get_game_file_safety_assessment'),
    );
    const repatchButton = await vi.waitFor(() => {
      const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
        (candidate) => candidate.getAttribute('aria-label') === 'Update all (2)',
      );
      if (!button || button.getAttribute('aria-disabled') === 'true') {
        throw new Error('Expected Update All to be available for the repatch');
      }
      return button;
    });
    repatchButton.click();
    flushSync();
    const repatchDialog = await dialogContaining(t('gameDetails.d3d12.confirm.title'));
    expect(repatchDialog.textContent.split(t('gameDetails.fileSafety.generic'))).toHaveLength(2);
    expect(repatchDialog.textContent).toContain('/games/test/Game.exe.rp-backup');
    expect(repatchDialog.querySelector('[data-slot="checkbox"]')).toBeNull();
    expect(repatchDialog.textContent).toContain(t('gameDetails.updateAll.action'));
    clickDialogButton(repatchDialog, t('gameDetails.updateAll.action'));
    await vi.waitFor(() => {
      expect(onBulkSwap).toHaveBeenCalledTimes(2);
    });
    expect(onBulkSwap).toHaveBeenLastCalledWith([
      expect.objectContaining({ confirmationToken: null, gameContextToken: 'game-safety-token' }),
    ]);
  });

  it('continues Update All after Developer Mode retry through one combined confirmation', async () => {
    safetyEnginesForGame = ['DeveloperRetryAntiCheat'];
    planSwapRepliesForTest = [
      {
        blockers: ['developer_mode_required'],
        confirmation_token: '',
        d3d12_executable_action: D3D12_ACTION,
      },
      {
        blockers: [],
        confirmation_token: 'retry-d3d12-token',
        d3d12_executable_action: D3D12_ACTION,
      },
    ];
    const onBulkSwap = vi.fn(() => Promise.resolve());
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: d3dUpdateDetails(), onBulkSwap },
    });
    flushSync();

    const assessmentRequestsBeforeAction = invokedCommands.filter(
      (command) => command === 'get_game_file_safety_assessment',
    ).length;
    const updateAll = await vi.waitFor(() => {
      const button = target.querySelector<HTMLButtonElement>('button[aria-label="Update all (1)"]');
      if (!button) {
        throw new Error('Expected the enabled Update All button');
      }
      return button;
    });
    updateAll.click();
    flushSync();

    const developerDialog = await dialogContaining(t('gameDetails.developerMode.requiredTitle'));
    clickDialogButton(developerDialog, t('gameDetails.developerMode.checkStatus'));
    flushSync();

    const riskDialog = await dialogContaining(t('gameDetails.fileSafety.generic'));
    const canonicalWarning = t('gameDetails.fileSafety.generic');
    expect(riskDialog.textContent.split(canonicalWarning)).toHaveLength(2);
    expect(riskDialog.textContent).toContain(t('gameDetails.d3d12.confirm.title'));
    expect(riskDialog.textContent).toContain('/games/test/Game.exe.rp-backup');
    expect(document.body.querySelectorAll('[role="dialog"]')).toHaveLength(1);
    clickDialogButton(riskDialog, t('gameDetails.updateAll.action'));

    await vi.waitFor(() => {
      expect(onBulkSwap).toHaveBeenCalledOnce();
    });
    expect(onBulkSwap).toHaveBeenCalledWith([
      expect.objectContaining({
        confirmationToken: 'retry-d3d12-token',
        gameContextToken: 'game-safety-token',
      }),
    ]);
    expect(invokedCommands.filter((command) => command === 'plan_swap')).toHaveLength(2);
    expect(
      invokedCommands.filter((command) => command === 'get_game_file_safety_assessment'),
    ).toHaveLength(assessmentRequestsBeforeAction + 1);
  });

  it('cancels the combined confirmation after Developer Mode retry without running Update All', async () => {
    safetyEnginesForGame = ['DeveloperRetryCancelAntiCheat'];
    planSwapRepliesForTest = [
      {
        blockers: ['developer_mode_required'],
        confirmation_token: '',
        d3d12_executable_action: D3D12_ACTION,
      },
      {
        blockers: [],
        confirmation_token: 'retry-d3d12-token',
        d3d12_executable_action: D3D12_ACTION,
      },
    ];
    const onBulkSwap = vi.fn(() => Promise.resolve());
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: d3dUpdateDetails(), onBulkSwap },
    });
    flushSync();

    const updateAll = await vi.waitFor(() => {
      const button = target.querySelector<HTMLButtonElement>('button[aria-label="Update all (1)"]');
      if (!button) {
        throw new Error('Expected the enabled Update All button');
      }
      return button;
    });
    updateAll.click();
    flushSync();

    const developerDialog = await dialogContaining(t('gameDetails.developerMode.requiredTitle'));
    clickDialogButton(developerDialog, t('gameDetails.developerMode.checkStatus'));
    flushSync();

    const riskDialog = await dialogContaining(t('gameDetails.fileSafety.generic'));
    clickDialogButton(riskDialog, t('common.cancel'));
    await vi.waitFor(() => {
      expect(riskDialog.isConnected).toBe(false);
    });

    expect(onBulkSwap).not.toHaveBeenCalled();
    expect(catalogSettingWrites).toEqual([]);
    expect(invokedCommands.filter((command) => command === 'plan_swap')).toHaveLength(2);
  });

  it('rejects an Update All add-on request that needs a wider scope than its captured token', async () => {
    safetyEnginesForGame = ['UpdateAllScopeTestMarker'];
    const renodxState = {
      status: 'installed' as const,
      host_kind: 'proxy' as 'proxy' | 'vulkan',
      version: 'snapshot-test',
      addon_dated: null,
      installed_at: 1,
      updated_at: 1,
      dlss_fix_evidence_present: false,
      addon_tracked: true,
    };
    renodxAvailabilityForTest = {
      ...unsupportedRenoDxAvailability(),
      state: renodxState,
      host_detection: 'present',
    };
    renodxCheckUpdateForTest = {
      addon: 'available',
      host: 'current',
      dlssFix: null,
      overall: 'available',
    };
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: createGameDetails({ addon_capabilities: ['renodx'] }) },
    });
    flushSync();

    const updateAll = await vi.waitFor(() => {
      const button = target.querySelector<HTMLButtonElement>('button[aria-label="Update all (1)"]');
      if (!button || button.getAttribute('aria-disabled') === 'true') {
        throw new Error('Expected the RenoDX Update All action to be available');
      }
      return button;
    });
    updateAll.click();
    flushSync();

    const dialog = await dialogContaining(t('gameDetails.fileSafety.generic'));
    expect(invokedCommands).not.toContain('get_shared_vulkan_safety_assessment');
    clickDialogButton(dialog, t('gameDetails.updateAll.action'));

    // Simulate the RenoDX store requiring shared Vulkan authority after the
    // batch has captured only the narrower game token.
    renodxState.host_kind = 'vulkan';
    await vi.waitFor(() => {
      const button = target.querySelector<HTMLButtonElement>('button[aria-label="Update all (1)"]');
      expect(button?.getAttribute('aria-busy')).toBe('false');
    });

    expect(getActiveNotifications()).toContainEqual(
      expect.objectContaining({
        title: t('gameDetails.renodx.updateError'),
        description: t('user_message.safety_context_scope_mismatch'),
      }),
    );
    expect(invokedCommands).not.toContain('get_shared_vulkan_safety_assessment');
    expect(invokedCommands).not.toContain('renodx_update');
  });

  it('stops the captured add-on batch when the game moves away and back during its first update', async () => {
    enableAddonUpdateScenarioForTest = true;
    safetyEnginesForGame = ['UpdateAllOwnerAntiCheat'];
    const lumaUpdate = deferred<unknown>();
    lumaUpdateForTest = lumaUpdate.promise;
    const onBulkSwap = vi.fn(() => Promise.resolve());
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: addonUpdateDetails('/games/first'), onBulkSwap },
    });
    flushSync();

    const updateAll = await vi.waitFor(() => {
      const button = target.querySelector<HTMLButtonElement>('button[aria-label="Update all (2)"]');
      if (!button) {
        throw new Error('Expected both add-on updates to be available');
      }
      return button;
    });
    updateAll.click();
    flushSync();

    const riskDialog = await dialogContaining(t('gameDetails.fileSafety.generic'));
    clickDialogButton(riskDialog, t('gameDetails.updateAll.action'));
    await vi.waitFor(() => {
      expect(invokedCommands).toContain('luma_update');
    });
    expect(invokedCommands).not.toContain('update_optiscaler');

    const host = component as {
      replaceDetails: (details: ReturnType<typeof createGameDetails>) => void;
    };
    const safetyRequestsBeforeMove = invokedCommands.filter(
      (command) => command === 'get_game_file_safety_assessment',
    ).length;
    host.replaceDetails(addonUpdateDetails('/games/second'));
    flushSync();
    const safetyRequestsAfterMove = invokedCommands.filter(
      (command) => command === 'get_game_file_safety_assessment',
    ).length;
    expect(safetyRequestsAfterMove).toBe(safetyRequestsBeforeMove);

    host.replaceDetails(addonUpdateDetails('/games/first'));
    flushSync();
    const safetyRequestsAfterReturn = invokedCommands.filter(
      (command) => command === 'get_game_file_safety_assessment',
    ).length;
    expect(safetyRequestsAfterReturn).toBe(safetyRequestsAfterMove);

    lumaUpdate.resolve({
      status: 'installed',
      version: 'Build 516',
      addon_dated: null,
      installed_at: 1,
      updated_at: 2,
      reshade_channel: 'nightly',
      launch_args: [],
    });
    await vi.waitFor(() => {
      expect(
        target
          .querySelector<HTMLButtonElement>('button[aria-label^="Update all"]')
          ?.getAttribute('aria-busy'),
      ).toBe('false');
    });

    expect(invokedCommands).not.toContain('update_optiscaler');
    expect(
      invokedCommands.filter((command) => command === 'get_game_file_safety_assessment'),
    ).toHaveLength(safetyRequestsAfterReturn);
    expect(
      [...document.body.querySelectorAll<HTMLElement>('[role="dialog"]')].some((dialog) =>
        dialog.textContent.includes(t('gameDetails.fileSafety.generic')),
      ),
    ).toBe(false);
    expect(getActiveNotifications()).toEqual([]);
  });

  it('does not duplicate a store-reported safety error with an Update All toast', async () => {
    enableAddonUpdateScenarioForTest = true;
    safetyEnginesForGame = ['UpdateAllStoreReportedAntiCheat'];
    lumaUpdateErrorForTest = DesktopCommandError.fromDto({ code: 'safety_context_stale' });
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: addonUpdateDetails('/games/test') },
    });
    flushSync();

    const updateAll = await vi.waitFor(() => {
      const button = target.querySelector<HTMLButtonElement>('button[aria-label="Update all (2)"]');
      if (!button) {
        throw new Error('Expected both add-on updates to be available');
      }
      return button;
    });
    updateAll.click();
    flushSync();

    const dialog = await dialogContaining(t('gameDetails.fileSafety.generic'));
    clickDialogButton(dialog, t('gameDetails.updateAll.action'));
    await vi.waitFor(() => {
      expect(invokedCommands).toContain('luma_update');
      expect(getActiveNotifications()).toHaveLength(1);
    });

    expect(invokedCommands).not.toContain('update_optiscaler');
    expect(getActiveNotifications().map((notification) => notification.title)).not.toContain(
      t('gameDetails.updateAll.partialFailure', { count: 1 }),
    );
  });

  it('reports a standalone swap safety assessment failure once', async () => {
    planSwapRepliesForTest = [
      {
        blockers: [],
        confirmation_token: 'd3d12-confirmation-token',
        d3d12_executable_action: D3D12_ACTION,
      },
    ];
    safetyAssessmentForTest = () =>
      Promise.reject(DesktopCommandError.fromDto({ code: 'safety_context_stale' }));
    const onSwap = vi.fn(() => Promise.resolve());
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: d3dUpdateDetails(), onSwap },
    });
    flushSync();

    const trigger = await vi.waitFor(() => {
      const button = target.querySelector<HTMLButtonElement>('button[aria-haspopup="listbox"]');
      if (!button) {
        throw new Error('Expected the D3D12 version selector');
      }
      return button;
    });
    await chooseSelectOption(trigger, 'artifact-d3d12');

    await vi.waitFor(() => {
      expect(getActiveNotifications()).toHaveLength(1);
    });
    expect(invokedCommands).toContain('get_game_file_safety_assessment');
    expect(onSwap).not.toHaveBeenCalled();
  });

  it('restores the installed version selection after cancelling a standalone EXE change', async () => {
    safetyEnginesForGame = ['BattlEye'];
    planSwapRepliesForTest = [
      {
        blockers: [],
        confirmation_token: 'd3d12-confirmation-token',
        d3d12_executable_action: D3D12_ACTION,
      },
    ];
    const onSwap = vi.fn(() => Promise.resolve());
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: d3dUpdateDetails(), onSwap },
    });
    flushSync();

    const trigger = await vi.waitFor(() => {
      const button = target.querySelector<HTMLButtonElement>('button[aria-haspopup="listbox"]');
      if (!button) {
        throw new Error('Expected the D3D12 version selector');
      }
      return button;
    });
    await chooseSelectOption(trigger, 'artifact-d3d12');
    const dialog = await dialogContaining('BattlEye');
    clickDialogButton(dialog, t('common.cancel'));

    await vi.waitFor(() => {
      expect(dialog.isConnected).toBe(false);
      expect(trigger.disabled).toBe(false);
    });
    expect(onSwap).not.toHaveBeenCalled();

    trigger.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 }));
    trigger.click();
    flushSync();
    const installed = await vi.waitFor(() => {
      const option = document.body.querySelector<HTMLElement>(
        '[role="option"][data-value="installed:component-d3d12:0"]',
      );
      if (!option) {
        throw new Error('Expected the installed version option');
      }
      return option;
    });
    expect(installed.getAttribute('aria-selected')).toBe('true');
    expect(
      document.body
        .querySelector('[role="option"][data-value="artifact-d3d12"]')
        ?.getAttribute('aria-selected'),
    ).not.toBe('true');
  });

  it('reports a standalone backend safety-token rejection once', async () => {
    safetyEnginesForGame = ['BattlEye'];
    planSwapRepliesForTest = [
      {
        blockers: [],
        confirmation_token: 'd3d12-confirmation-token',
        d3d12_executable_action: D3D12_ACTION,
      },
    ];
    const onSwap = vi.fn(() =>
      Promise.reject(DesktopCommandError.fromDto({ code: 'safety_context_stale' })),
    );
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: d3dUpdateDetails(), onSwap },
    });
    flushSync();

    const trigger = await vi.waitFor(() => {
      const button = target.querySelector<HTMLButtonElement>('button[aria-haspopup="listbox"]');
      if (!button) {
        throw new Error('Expected the D3D12 version selector');
      }
      return button;
    });
    await chooseSelectOption(trigger, 'artifact-d3d12');
    const dialog = await dialogContaining('BattlEye');
    clickDialogButton(dialog, t('gameDetails.fileSafety.confirmExePatchAction'));

    await vi.waitFor(() => {
      expect(getActiveNotifications()).toHaveLength(1);
    });
    expect(onSwap).toHaveBeenCalledOnce();
    expect(invokedCommands).toContain('get_game_file_safety_assessment');
  });

  it('shows a preparation failure for same-owner safety capture errors', async () => {
    enableAddonUpdateScenarioForTest = true;
    safetyAssessmentForTest = () =>
      Promise.reject(DesktopCommandError.fromDto({ code: 'safety_context_stale' }));
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: addonUpdateDetails('/games/test') },
    });
    flushSync();

    const updateAll = await vi.waitFor(() => {
      const button = target.querySelector<HTMLButtonElement>('button[aria-label="Update all (2)"]');
      if (!button) {
        throw new Error('Expected both add-on updates to be available');
      }
      return button;
    });
    updateAll.click();

    await vi.waitFor(() => {
      expect(getActiveNotifications()).toHaveLength(1);
    });
    expect(getActiveNotifications()[0]?.title).toBe(t('gameDetails.updateAll.prepareFailed'));
    expect(invokedCommands).toContain('get_game_file_safety_assessment');
    expect(invokedCommands).not.toContain('luma_update');
    expect(invokedCommands).not.toContain('update_optiscaler');
    expect(target.querySelector('[role="dialog"]')).toBeNull();
  });

  it('silently discards a late safety assessment after the Update All owner changes', async () => {
    enableAddonUpdateScenarioForTest = true;
    const assessment = deferred<unknown>();
    safetyAssessmentForTest = () => assessment.promise;
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: addonUpdateDetails('/games/first') },
    });
    flushSync();

    const updateAll = await vi.waitFor(() => {
      const button = target.querySelector<HTMLButtonElement>('button[aria-label="Update all (2)"]');
      if (!button) {
        throw new Error('Expected both add-on updates to be available');
      }
      return button;
    });
    updateAll.click();
    await vi.waitFor(() => {
      expect(invokedCommands).toContain('get_game_file_safety_assessment');
    });

    const host = component as {
      replaceDetails: (details: ReturnType<typeof createGameDetails>) => void;
    };
    host.replaceDetails(addonUpdateDetails('/games/second'));
    flushSync();
    assessment.resolve({
      game_id: 'steam:123',
      context_token: 'stale-game-safety-token',
      detected_engines: [],
      scan_completeness: 'complete',
    });

    await vi.waitFor(() => {
      expect(target.querySelector('[role="dialog"]')).toBeNull();
      expect(getActiveNotifications()).toEqual([]);
      expect(invokedCommands).not.toContain('luma_update');
    });
  });

  it('combines the fresh risk notice and single-swap executable plan, then keeps EXE-only confirmation', async () => {
    await setLanguageMode('ru');
    safetyEnginesForGame = ['BattlEye'];
    safetyCompletenessForGame = 'limited';
    planSwapRepliesForTest = [
      {
        blockers: [],
        confirmation_token: 'first-patch-token',
        d3d12_executable_action: D3D12_ACTION,
      },
      {
        blockers: [],
        confirmation_token: '',
        d3d12_executable_action: D3D12_REPATCH_ACTION,
      },
      {
        blockers: [],
        confirmation_token: 'must-not-be-forwarded',
        d3d12_executable_action: D3D12_REPATCH_ACTION,
      },
    ];
    const onSwap = vi.fn(() => Promise.resolve());
    const candidates = ['artifact-1', 'artifact-2'].map((artifactId, index) => ({
      artifact_id: artifactId,
      file_name: 'Game.dll',
      file_path: null,
      technical_version: `${index + 2}.0`,
      release_label: null,
      source_game_id: null,
      comparison: 'newer',
      catalog_package: null,
      is_downloaded: true,
      is_debug: false,
      sha256: `sha256-${artifactId}`,
      d3d12_executable_action: index === 0 ? D3D12_ACTION : D3D12_REPATCH_ACTION,
    }));
    const details = createGameDetails({
      game: { identity: { id: 'steam:123', title: 'Test Game', launcher: 'Steam' } },
      components: [
        {
          id: 'component-d3d12',
          game_id: 'steam:123',
          kind: 'library',
          technology: 'd3d12_agility',
          swappability: 'swappable',
          files: [],
          rollback_available: false,
          d3d12_executable_status: null,
        },
      ],
      candidate_groups: [
        {
          component_id: 'component-d3d12',
          technology: 'd3d12_agility',
          file_path: 'Game.dll',
          version_report: {
            kind: 'known',
            technical_version: '1.0',
            release_label: null,
            catalog_release: null,
          },
          automatic_candidate_artifact_id: null,
          candidates,
        },
      ],
    });
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details, onSwap },
    });
    flushSync();

    async function choose(artifactId: string): Promise<HTMLElement> {
      const trigger = await vi.waitFor(() => {
        const candidate = [
          ...target.querySelectorAll<HTMLButtonElement>('button[aria-haspopup="listbox"]'),
        ].at(0);
        if (!candidate) {
          throw new Error('Expected the D3D12 version selector');
        }
        return candidate;
      });
      trigger.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 }));
      trigger.click();
      flushSync();
      const option = await vi.waitFor(() => {
        const candidate = document.body.querySelector<HTMLElement>(
          `[role="option"][data-value="${artifactId}"]`,
        );
        if (!candidate) {
          throw new Error(`Expected candidate ${artifactId}`);
        }
        return candidate;
      });
      option.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 }));
      option.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, button: 0 }));
      option.click();
      flushSync();
      return option;
    }

    await choose('artifact-1');
    const combinedDialog = await vi.waitFor(() => {
      const dialog = [...document.body.querySelectorAll<HTMLElement>('[role="dialog"]')].find(
        (item) => item.textContent.includes('BattlEye'),
      );
      if (!dialog) {
        throw new Error('Expected a combined risk and executable confirmation');
      }
      return dialog;
    });
    const canonicalWarning = t('gameDetails.fileSafety.generic');
    expect(combinedDialog.textContent.split(canonicalWarning)).toHaveLength(2);
    expect(combinedDialog.textContent.split('BattlEye')).toHaveLength(2);
    expect(combinedDialog.textContent).toContain(t('gameDetails.fileSafety.detectedEngines'));
    expect(combinedDialog.textContent).not.toContain(t('gameDetails.fileSafety.limitedDetail'));
    expect(combinedDialog.textContent).toContain(t('gameDetails.d3d12.confirm.title'));
    expect(combinedDialog.textContent).not.toContain('D3D12SDKVersion');
    expect(combinedDialog.textContent).toContain(
      t('gameDetails.d3d12.action.planPatch', { from: 606, to: 619 }),
    );
    expect(combinedDialog.textContent).toContain('/games/test/Game.exe.rp-backup');
    expect(combinedDialog.textContent).toContain(t('gameDetails.d3d12.confirm.signatureWarning'));
    expect(combinedDialog.querySelector('[data-slot="dialog-title"]')?.textContent).toBe(
      t('gameDetails.d3d12.confirm.title'),
    );
    expect(combinedDialog.textContent).toContain(t('gameDetails.fileSafety.confirmExePatchAction'));
    expect(document.body.querySelectorAll('[role="dialog"]')).toHaveLength(1);
    [...combinedDialog.querySelectorAll<HTMLButtonElement>('button')]
      .find((button) =>
        button.textContent.includes(t('gameDetails.fileSafety.confirmExePatchAction')),
      )
      ?.click();
    await vi.waitFor(() => {
      expect(onSwap).toHaveBeenCalledOnce();
    });
    expect(onSwap).toHaveBeenLastCalledWith(
      expect.objectContaining({
        artifactId: 'artifact-1',
        confirmationToken: 'first-patch-token',
        gameContextToken: 'game-safety-token',
      }),
    );

    await choose('artifact-2');
    const executableOnlyDialog = await vi.waitFor(() => {
      const dialog = document.body.querySelector<HTMLElement>('[role="dialog"]');
      if (
        !dialog ||
        !dialog.textContent.includes(t('gameDetails.d3d12.confirm.title')) ||
        !dialog.textContent.includes('BattlEye')
      ) {
        throw new Error('Expected the detected-engine warning with the executable confirmation');
      }
      return dialog;
    });
    expect(executableOnlyDialog.textContent).toContain('/games/test/Game.exe.rp-backup');
    expect(executableOnlyDialog.textContent.split(canonicalWarning)).toHaveLength(2);
    expect(executableOnlyDialog.textContent).toContain(t('gameDetails.d3d12.confirm.title'));
    expect(executableOnlyDialog.textContent).toContain(
      t('gameDetails.fileSafety.confirmExePatchAction'),
    );
    expect(executableOnlyDialog.textContent).toContain(t('gameDetails.fileSafety.detectedEngines'));
    expect(executableOnlyDialog.textContent).not.toContain(
      t('gameDetails.fileSafety.limitedDetail'),
    );
    expect(executableOnlyDialog.querySelector('[data-slot="checkbox"]')).toBeNull();
    [...executableOnlyDialog.querySelectorAll<HTMLButtonElement>('button')]
      .find((button) =>
        button.textContent.includes(t('gameDetails.fileSafety.confirmExePatchAction')),
      )
      ?.click();
    await vi.waitFor(() => {
      expect(onSwap).toHaveBeenCalledTimes(2);
    });
    expect(onSwap).toHaveBeenLastCalledWith(
      expect.objectContaining({ artifactId: 'artifact-2', confirmationToken: null }),
    );
  });

  it('treats standalone Streamline confirmation cancellation as a quiet skipped swap', async () => {
    safetyEnginesForGame = ['EasyAntiCheat-Streamline'];
    const onBulkSwap = vi.fn(() => Promise.resolve());
    const optionId = 'a'.repeat(64);

    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: streamlineUpdateDetails(optionId), onBulkSwap },
    });
    flushSync();

    const trigger = await vi.waitFor(() => {
      const candidate = [
        ...target.querySelectorAll<HTMLButtonElement>('button[aria-haspopup="listbox"]'),
      ].find((button) => button.textContent.includes('2.3.0'));
      if (!candidate) {
        throw new Error('Expected the Streamline version selector');
      }
      return candidate;
    });
    await chooseSelectOption(trigger, optionId);

    const dialog = await vi.waitFor(() => {
      const candidate = [...document.body.querySelectorAll<HTMLElement>('[role="dialog"]')].find(
        (item) => item.textContent.includes('EasyAntiCheat-Streamline'),
      );
      if (!candidate) {
        throw new Error('Expected the anti-cheat game-file risk confirmation');
      }
      return candidate;
    });
    expect(dialog.querySelector('[data-slot="dialog-title"]')?.textContent).toBe(
      t('gameDetails.fileSafety.confirmTitle'),
    );
    expect(dialog.textContent).toContain(t('gameDetails.fileSafety.confirmChangeAction'));
    [...dialog.querySelectorAll<HTMLButtonElement>('button')]
      .find((button) => button.textContent.includes('Cancel'))
      ?.click();

    await vi.waitFor(() => {
      expect(dialog.isConnected).toBe(false);
    });
    expect(onBulkSwap).not.toHaveBeenCalled();
    await vi.waitFor(() => {
      expect(trigger.disabled).toBe(false);
    });
    trigger.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 }));
    trigger.click();
    flushSync();
    const cancelledOption = await vi.waitFor(() => {
      const option = document.body.querySelector<HTMLElement>(
        `[role="option"][data-value="${optionId}"]`,
      );
      if (!option) {
        throw new Error('Expected the Streamline version option');
      }
      return option;
    });
    expect(cancelledOption.getAttribute('aria-selected')).not.toBe('true');
    expect(getActiveNotifications()).toEqual([]);
    expect(
      invokedCommands.filter((command) => command === 'get_game_file_safety_assessment'),
    ).toHaveLength(1);
  });

  it('reports standalone Streamline safety-token failures once', async () => {
    safetyEnginesForGame = ['EasyAntiCheat-Streamline'];
    const onBulkSwap = vi.fn(() =>
      Promise.reject(DesktopCommandError.fromDto({ code: 'safety_context_stale' })),
    );
    const optionId = 'b'.repeat(64);
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: streamlineUpdateDetails(optionId), onBulkSwap },
    });
    flushSync();

    const trigger = await vi.waitFor(() => {
      const button = [
        ...target.querySelectorAll<HTMLButtonElement>('button[aria-haspopup="listbox"]'),
      ].find((candidate) => candidate.textContent.includes('2.3.0'));
      if (!button) {
        throw new Error('Expected the Streamline version selector');
      }
      return button;
    });
    await chooseSelectOption(trigger, optionId);

    const dialog = await dialogContaining('EasyAntiCheat-Streamline');
    clickDialogButton(dialog, t('gameDetails.fileSafety.confirmChangeAction'));
    await vi.waitFor(() => {
      expect(onBulkSwap).toHaveBeenCalledOnce();
      expect(getActiveNotifications()).toHaveLength(1);
    });

    expect(getActiveNotifications()).toContainEqual(
      expect.objectContaining({ description: t('user_message.safety_context_stale') }),
    );
  });

  it('does not passively assess file safety on mount', async () => {
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: createGameDetails() },
    });
    flushSync();

    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(invokedCommands).not.toContain('get_game_file_safety_assessment');
    expect(invokedCommands).not.toContain('get_shared_vulkan_safety_assessment');
    expect(
      [...target.querySelectorAll<HTMLButtonElement>('button')].some((button) =>
        button.textContent.includes('Risk'),
      ),
    ).toBe(false);
    expect(target.querySelector('[data-file-safety-row]')).toBeNull();
  });

  it.each([
    {
      capabilities: [] as const,
      hasAddonsTab: false,
      hasRenoDx: false,
      hasLuma: false,
      hasOptiScaler: false,
    },
    {
      capabilities: ['renodx'] as const,
      hasAddonsTab: true,
      hasRenoDx: true,
      hasLuma: false,
      hasOptiScaler: false,
    },
    {
      capabilities: ['luma'] as const,
      hasAddonsTab: true,
      hasRenoDx: false,
      hasLuma: true,
      hasOptiScaler: false,
    },
    {
      capabilities: ['renodx', 'luma'] as const,
      hasAddonsTab: true,
      hasRenoDx: true,
      hasLuma: true,
      hasOptiScaler: false,
    },
    {
      capabilities: ['optiscaler'] as const,
      hasAddonsTab: true,
      hasRenoDx: false,
      hasLuma: false,
      hasOptiScaler: true,
    },
  ])(
    'gates the add-on tab, cards, and availability for $capabilities',
    async ({ capabilities, hasAddonsTab, hasRenoDx, hasLuma, hasOptiScaler }) => {
      component = mount(GameDetailsPageTestHost, {
        target,
        props: {
          details: createGameDetails({ addon_capabilities: [...capabilities] }),
        },
      });
      flushSync();

      await vi.waitFor(() => {
        expect(invokedCommands.includes('renodx_availability')).toBe(hasRenoDx);
        expect(invokedCommands.includes('luma_availability')).toBe(hasLuma);
        expect(invokedCommands.includes('get_optiscaler_availability')).toBe(hasOptiScaler);
      });

      const text = target.textContent;
      const tabLabels = [...target.querySelectorAll<HTMLElement>('[role="tab"]')].map((tab) =>
        tab.textContent.trim(),
      );
      expect(tabLabels.includes('Addons')).toBe(hasAddonsTab);
      expect(text.includes('RenoDX HDR')).toBe(hasRenoDx);
      expect(text.includes('Luma Framework')).toBe(hasLuma);
      expect(text.includes('OptiScaler')).toBe(hasOptiScaler);
    },
  );

  it('does not reload add-on stores when same-game details keep the same capabilities', async () => {
    component = mount(GameDetailsPageTestHost, {
      target,
      props: {
        details: createGameDetails({ addon_capabilities: ['renodx'] }),
      },
    });
    flushSync();
    await vi.waitFor(() => {
      expect(invokedCommands.filter((command) => command === 'renodx_availability')).toHaveLength(
        1,
      );
    });

    const host = component as {
      replaceDetails: (details: ReturnType<typeof createGameDetails>) => void;
    };
    host.replaceDetails(
      createGameDetails({
        addon_capabilities: ['renodx'],
        operations: [
          {
            operation_id: 'operation-1',
            kind: 'swap',
            status: 'completed',
            created_at: 1,
            completed_at: 2,
            item_count: 1,
            component_id: 'component-1',
            metadata: null,
          },
        ],
      }),
    );
    flushSync();
    await Promise.resolve();

    expect(invokedCommands.filter((command) => command === 'renodx_availability')).toHaveLength(1);
  });

  it('uses the normalized game ID in the add-on activation signature', async () => {
    component = mount(GameDetailsPageTestHost, {
      target,
      props: {
        details: createGameDetails({
          game: {
            identity: { id: '  game-1  ', title: 'Test Game', launcher: 'Manual' },
            platform: 'Windows',
            runtime: 'NativeWindows',
            install_path: '/test',
            can_remove_from_catalog: true,
          },
          addon_capabilities: ['renodx'],
        }),
      },
    });
    flushSync();
    await vi.waitFor(() => {
      expect(invokedCommands.filter((command) => command === 'renodx_availability')).toHaveLength(
        1,
      );
    });

    const host = component as {
      replaceDetails: (details: ReturnType<typeof createGameDetails>) => void;
    };
    host.replaceDetails(
      createGameDetails({
        game: {
          identity: { id: 'game-1', title: 'Renamed Game', launcher: 'Manual' },
          platform: 'Windows',
          runtime: 'NativeWindows',
          install_path: '/test',
          can_remove_from_catalog: true,
        },
        addon_capabilities: ['renodx', 'renodx'],
      }),
    );
    flushSync();
    await Promise.resolve();

    expect(invokedCommands.filter((command) => command === 'renodx_availability')).toHaveLength(1);
  });

  it('deactivates a removed capability and reloads it when the capability returns', async () => {
    component = mount(GameDetailsPageTestHost, {
      target,
      props: {
        details: createGameDetails({ addon_capabilities: ['renodx'] }),
      },
    });
    flushSync();
    await vi.waitFor(() => {
      expect(invokedCommands.filter((command) => command === 'renodx_availability')).toHaveLength(
        1,
      );
    });

    const host = component as {
      replaceDetails: (details: ReturnType<typeof createGameDetails>) => void;
    };
    host.replaceDetails(createGameDetails({ addon_capabilities: [] }));
    flushSync();

    expect(target.textContent).not.toContain('RenoDX HDR');
    expect(target.textContent).not.toContain('Addons');

    host.replaceDetails(createGameDetails({ addon_capabilities: ['renodx'] }));
    flushSync();
    await vi.waitFor(() => {
      expect(invokedCommands.filter((command) => command === 'renodx_availability')).toHaveLength(
        2,
      );
    });

    expect(target.textContent).toContain('RenoDX HDR');
    expect(target.textContent).toContain('Addons');
  });

  it('does not start a passive safety assessment when the selected game changes', async () => {
    disposeInvoker?.();
    const safetyGameIds: string[] = [];
    const invoker = ((command: string, payload?: Record<string, unknown>) => {
      invokedCommands.push(command);
      if (command === 'get_game_file_safety_assessment') {
        const requestedGameId = typeof payload?.gameId === 'string' ? payload.gameId : '';
        safetyGameIds.push(requestedGameId);
        return Promise.resolve({
          game_id: requestedGameId,
          context_token: `${requestedGameId}-safety-token`,
          detected_engines: ['easy_anti_cheat'],
          scan_completeness: 'complete',
        });
      }
      if (command === 'get_shared_vulkan_safety_assessment') {
        return Promise.resolve({ context_token: 'shared-vulkan-safety-token' });
      }
      if (command === 'resolve_game_executable') {
        return Promise.resolve(null);
      }
      if (command === 'list_game_executable_candidates') {
        return Promise.resolve([]);
      }
      if (command === 'get_nvapi_profile_status') {
        return Promise.resolve({
          selectedExecutable: null,
          bindingPath: null,
          profileName: null,
          state: 'noExecutable',
          isPredefined: null,
          ownedByThisGame: false,
          canCreate: false,
          canDelete: false,
          pendingOperation: null,
          pendingOperationGameId: null,
          detail: null,
        });
      }
      unexpectedCommands.push(command);
      return Promise.reject(new Error(`Unexpected command in game-switch test: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    const detailsFor = (gameId: string) =>
      createGameDetails({
        game: {
          identity: { id: gameId, title: gameId, launcher: 'Manual' },
          platform: 'Windows',
          runtime: 'NativeWindows',
          install_path: `/test/${gameId}`,
          can_remove_from_catalog: true,
        },
      });
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: detailsFor('game-1') },
    });
    flushSync();
    expect(safetyGameIds).toEqual([]);

    const host = component as {
      replaceDetails: (details: ReturnType<typeof createGameDetails>) => void;
    };
    host.replaceDetails(detailsFor('game-2'));
    flushSync();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(safetyGameIds).toEqual([]);
  });
});
