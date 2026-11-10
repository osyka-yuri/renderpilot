/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { createGameDetails, type GameCandidateGroup, type GameDetails } from '@entities/game';
import type { SettingStateResponse } from '@features/nvapi-settings';
import { setLanguageMode } from '@shared/i18n';
import type { NvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';
import type { SwapHandler } from '../model/create-game-details-page-model';

import GameDetailsTabsContentTestHost from './GameDetailsTabsContent.nvidia.test-host.svelte';

const INSTALL_PATH = 'C:/Games/Modern Warfare II';
const CAMPAIGN_PATH = `${INSTALL_PATH}/campaign/nvngx_dlss.dll`;
const MULTIPLAYER_PATH = `${INSTALL_PATH}/multiplayer/nvngx_dlss.dll`;
const EXECUTABLE_PATH = `${INSTALL_PATH}/game.exe`;

type TestNvidiaContextOptions = {
  readiness?: SettingStateResponse['catalog_readiness'];
  includeNonDllSetting?: boolean;
  includeManifest?: boolean;
};

function setting(overrides: Partial<SettingStateResponse> = {}): SettingStateResponse {
  return {
    setting_key: 'dlss_sr_render_preset',
    setting_label: 'DLSS render preset',
    value_type: 'dword',
    dll_kind: 'sr',
    family: 'sr',
    category: 'DLSS Super Resolution',
    description: null,
    min_driver: null,
    current: { wire: 'balanced', label: 'Balanced', dword: 3 },
    current_is_explicit: true,
    predefined: { wire: 'default', label: 'Default', dword: 0 },
    original: null,
    is_current_predefined: false,
    effective_exe: EXECUTABLE_PATH,
    effective_exe_source: 'auto',
    has_profile_for_exe: true,
    nvapi_available: true,
    catalog_readiness: 'ready',
    available_values: [
      { wire: 'balanced', label: 'Balanced', supported: true },
      { wire: 'quality', label: 'Quality', supported: true },
    ],
    dll_info: {
      kind: 'sr',
      version: '3.7.20',
      path: CAMPAIGN_PATH,
      manifest_label: 'DLSS 3.7.20',
    },
    warnings: [],
    ...overrides,
  };
}

function testNvidiaContext(options: TestNvidiaContextOptions = {}): {
  context: NvidiaDriverContext;
  setValue: ReturnType<typeof vi.fn>;
} {
  const preset = setting({ catalog_readiness: options.readiness ?? 'ready' });
  if (options.includeManifest === false && preset.dll_info) {
    preset.dll_info.manifest_label = null;
  }

  const settings = [preset];
  if (options.includeNonDllSetting) {
    settings.push(
      setting({
        setting_key: 'dlss_sr_shared_setting',
        setting_label: 'Non-DLL profile setting',
        dll_kind: null,
        current: { wire: 'enabled', label: 'Enabled', dword: 1 },
        current_is_explicit: false,
        predefined: null,
        catalog_readiness: 'notApplicable',
        available_values: [{ wire: 'disabled', label: 'Disabled', supported: true }],
      }),
    );
  }

  const setValue = vi.fn(() => Promise.resolve());
  const context = {
    loaded: true,
    effectiveExe: EXECUTABLE_PATH,
    nvapiAvailable: true,
    busy: false,
    settingsForFamily: (family: string) => (family === 'sr' ? settings : []),
    familyWarnings: () => [],
    dllInfoForFamily: () => preset.dll_info,
    isPending: () => false,
    setValue,
    revert: vi.fn(() => Promise.resolve()),
  } as unknown as NvidiaDriverContext;

  return { context, setValue };
}

function libraryComponent(id: string, path: string, version: string) {
  return {
    id,
    game_id: 'game:test',
    kind: 'native_library',
    technology: 'dlss_super_resolution',
    swappability: 'swappable',
    files: [{ path, version }],
    rollback_available: false,
    d3d12_executable_status: null,
  };
}

function candidateGroup(
  componentId: string,
  path: string,
  artifactId: string,
  version: string,
): GameCandidateGroup {
  return {
    component_id: componentId,
    technology: 'dlss_super_resolution',
    file_path: path,
    version_report: { kind: 'unknown' },
    automatic_candidate_artifact_id: null,
    candidates: [
      {
        artifact_id: artifactId,
        file_name: 'nvngx_dlss.dll',
        file_path: null,
        technical_version: version,
        release_label: null,
        source_game_id: null,
        comparison: 'newer_version',
        catalog_package: null,
        is_downloaded: true,
        is_debug: false,
        sha256: `${artifactId}-hash`,
        d3d12_executable_action: null,
      },
    ],
  };
}

function detailsWithTwoDlssFiles(): GameDetails {
  const campaignId = 'component:dlss-campaign';
  const multiplayerId = 'component:dlss-multiplayer';

  return createGameDetails({
    game: {
      identity: { id: 'game:test', title: 'Modern Warfare II', launcher: 'Steam' },
      platform: 'Windows',
      runtime: 'NativeWindows',
      install_path: INSTALL_PATH,
    },
    components: [
      libraryComponent(campaignId, CAMPAIGN_PATH, '3.7.20'),
      libraryComponent(multiplayerId, MULTIPLAYER_PATH, '3.6.10'),
    ],
    candidate_groups: [
      candidateGroup(campaignId, CAMPAIGN_PATH, 'campaign-dlss-next', '3.8.0'),
      candidateGroup(multiplayerId, MULTIPLAYER_PATH, 'multiplayer-dlss-next', '3.7.0'),
    ],
  });
}

async function chooseSelectOption(trigger: HTMLButtonElement, value: string): Promise<void> {
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

function buttonWithText(target: HTMLElement, text: string): HTMLButtonElement {
  const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find((candidate) =>
    candidate.textContent.includes(text),
  );
  if (!button) {
    throw new Error(`Expected button containing: ${text}`);
  }
  return button;
}

function settingSelectWithText(target: HTMLElement, text: string): HTMLButtonElement {
  const button = [
    ...target.querySelectorAll<HTMLButtonElement>(
      '[data-slot="accordion-content"] [data-slot="select-trigger"]',
    ),
  ].find((candidate) => candidate.textContent.includes(text));
  if (!button) {
    throw new Error(`Expected NVIDIA setting selector containing: ${text}`);
  }
  return button;
}

describe('GameDetailsPage NVIDIA DLSS family integration', () => {
  let target: HTMLDivElement;
  let mounted: object | undefined;

  beforeEach(async () => {
    await setLanguageMode('en');
    Object.defineProperty(window, 'ResizeObserver', {
      configurable: true,
      value: class ResizeObserverMock {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      },
    });
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (mounted) {
      await unmount(mounted);
      mounted = undefined;
    }
    target.remove();
  });

  function render(options: TestNvidiaContextOptions = {}, onSwap: SwapHandler = vi.fn()) {
    const { context, setValue } = testNvidiaContext(options);
    mounted = mount(GameDetailsTabsContentTestHost, {
      target,
      props: { details: detailsWithTwoDlssFiles(), nvidia: context, onSwap },
    });
    flushSync();
    return { nvidia: context, setValue, onSwap };
  }

  function openOverrides(): HTMLButtonElement {
    const trigger = buttonWithText(target, 'NVIDIA settings');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    trigger.click();
    flushSync();
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    return trigger;
  }

  it('groups two SR DLLs into one card with distinct install-relative paths and versions', () => {
    render();

    expect(target.querySelectorAll('[data-slot="card"]')).toHaveLength(1);
    expect(target.querySelectorAll('[data-slot="card-title"]')).toHaveLength(1);
    expect(target.querySelector('[data-slot="card-title"]')?.textContent).toBe(
      'NVIDIA DLSS Super Resolution',
    );
    const fileRows = [...target.querySelectorAll<HTMLElement>('[data-slot="item"]')].filter((row) =>
      row.textContent.includes('nvngx_dlss.dll'),
    );
    expect(fileRows).toHaveLength(2);
    expect(target.textContent).toContain('campaign/nvngx_dlss.dll');
    expect(target.textContent).toContain('multiplayer/nvngx_dlss.dll');
    expect(target.textContent).toContain('v3.7.20');
    expect(target.textContent).toContain('v3.6.10');

    const trigger = buttonWithText(target, 'NVIDIA settings');
    expect(target.querySelectorAll('[data-slot="accordion-trigger"]')).toHaveLength(1);
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(trigger.textContent.trim()).toBe('NVIDIA settings');

    openOverrides();
    expect(target.textContent).toContain(
      'Preset support is checked against campaign/nvngx_dlss.dll. It may differ for other copies.',
    );
    expect(
      target.querySelectorAll('[data-slot="accordion-content"] [data-slot="select-trigger"]'),
    ).toHaveLength(1);
  });

  it('does not claim preset compatibility when the DLL reference has no manifest', () => {
    render({ includeManifest: false });
    openOverrides();

    expect(target.textContent).not.toContain('Preset support is checked against');
  });

  it('keeps candidate replacement attached to each component and shares one live preset write', async () => {
    const onSwap = vi.fn<SwapHandler>();
    const { setValue } = render({}, onSwap);

    const campaignVersion = buttonWithText(target, 'v3.7.20');
    await chooseSelectOption(campaignVersion, 'campaign-dlss-next');
    const multiplayerVersion = buttonWithText(target, 'v3.6.10');
    await chooseSelectOption(multiplayerVersion, 'multiplayer-dlss-next');

    expect(onSwap).toHaveBeenCalledTimes(2);
    expect(onSwap.mock.calls.map(([request]) => [request.componentId, request.artifactId])).toEqual(
      [
        ['component:dlss-campaign', 'campaign-dlss-next'],
        ['component:dlss-multiplayer', 'multiplayer-dlss-next'],
      ],
    );

    openOverrides();
    const settingTrigger = settingSelectWithText(target, 'Balanced');
    await chooseSelectOption(settingTrigger, 'quality');
    await vi.waitFor(() => {
      expect(setValue).toHaveBeenCalledTimes(1);
    });
    expect(setValue).toHaveBeenCalledWith('game:test', 'dlss_sr_render_preset', 'quality');
  });

  it('blocks DLL-dependent settings until catalog readiness while preserving non-DLL and file actions', () => {
    render({ readiness: 'notReady', includeNonDllSetting: true });
    openOverrides();

    expect(settingSelectWithText(target, 'Balanced').disabled).toBe(true);
    expect(settingSelectWithText(target, 'Enabled').disabled).toBe(false);
    expect(buttonWithText(target, 'v3.7.20').disabled).toBe(false);
    expect(buttonWithText(target, 'v3.6.10').disabled).toBe(false);
  });
});
