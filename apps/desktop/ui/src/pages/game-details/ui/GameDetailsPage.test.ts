/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { defaultHostFacts } from '@entities/addon';
import { createGameDetails } from '@entities/game';
import type { SettingStateResponse } from '@features/nvapi-settings';
import { registerPreviewInvoker, type DesktopInvoker } from '@shared/api-preview';

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

let nvapiProfileStatusForTest: Record<string, unknown>;

function unsupportedRenoDxAvailability(): unknown {
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

describe('GameDetailsPage', () => {
  let target: HTMLDivElement;
  let component: object | undefined;
  let disposeInvoker: (() => void) | undefined;
  let invokedCommands: string[];
  let unexpectedCommands: string[];
  let executableOverrideGameIds: string[];
  let profileStatusForGame: (gameId: string) => Promise<unknown>;
  let executableForGame: (gameId: string) => Promise<unknown>;
  let executableCandidatesForGame: (gameId: string) => Promise<unknown>;
  let nvidiaSettingsForGame: (gameId: string) => Promise<SettingStateResponse[]>;
  let deleteProfileForGame: (gameId: string) => Promise<unknown>;

  beforeEach(() => {
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
    invokedCommands = [];
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
        return Promise.resolve(unsupportedRenoDxAvailability());
      }
      if (command === 'luma_availability') {
        return Promise.resolve(unsupportedLumaAvailability());
      }
      if (command === 'get_optiscaler_availability') {
        return Promise.resolve(unsupportedOptiScalerAvailability());
      }
      if (command === 'renodx_dlss_fix_availability') {
        return Promise.resolve(unavailableDlssFixAvailability());
      }
      if (command === 'get_game_file_safety_assessment') {
        return Promise.resolve({
          game_id: 'steam:123',
          context_token: 'game-safety-token',
          detected_engines: [],
          scan_completeness: 'complete',
        });
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

  it('loads only the game assessment for the passive safety notice', async () => {
    component = mount(GameDetailsPageTestHost, {
      target,
      props: { details: createGameDetails() },
    });
    flushSync();

    await vi.waitFor(() => {
      expect(invokedCommands).toContain('get_game_file_safety_assessment');
    });
    expect(invokedCommands).not.toContain('get_shared_vulkan_safety_assessment');
    const safetyRows = target.querySelectorAll('[data-file-safety-row]');
    expect(safetyRows).toHaveLength(1);
    expect(safetyRows[0]?.closest('[data-slot="scroll-area-viewport"]')).not.toBeNull();
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

  it('starts a fresh safety assessment when the selected game changes mid-request', async () => {
    disposeInvoker?.();
    let resolveFirstAssessment!: (assessment: unknown) => void;
    const firstAssessment = new Promise<unknown>((resolve) => {
      resolveFirstAssessment = resolve;
    });
    const safetyGameIds: string[] = [];
    const invoker = ((command: string, payload?: Record<string, unknown>) => {
      invokedCommands.push(command);
      if (command === 'get_game_file_safety_assessment') {
        const requestedGameId = typeof payload?.gameId === 'string' ? payload.gameId : '';
        safetyGameIds.push(requestedGameId);
        if (requestedGameId === 'game-1') {
          return firstAssessment;
        }
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
    await vi.waitFor(() => {
      expect(safetyGameIds).toEqual(['game-1']);
    });

    const host = component as {
      replaceDetails: (details: ReturnType<typeof createGameDetails>) => void;
    };
    host.replaceDetails(detailsFor('game-2'));
    flushSync();
    resolveFirstAssessment({
      game_id: 'game-1',
      context_token: 'game-1-safety-token',
      detected_engines: [],
      scan_completeness: 'complete',
    });

    await vi.waitFor(() => {
      expect(safetyGameIds).toEqual(['game-1', 'game-2']);
      expect(target.textContent).toContain('Easy Anti-Cheat detected.');
    });
  });
});
