/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { defaultHostFacts } from '@entities/addon';
import { createGameDetails } from '@entities/game';
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

function unsupportedRenoDxAvailability(): unknown {
  return {
    state: { status: 'not_installed' },
    host_detection: 'absent',
    host_facts: defaultHostFacts('stable'),
    actions: {},
    reshade_stable_supported: true,
    renodx_addon: null,
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
    outcome: { kind: 'unsupported' },
  };
}

function unavailableDlssFixAvailability(): unknown {
  return {
    kind: 'binding',
    state: 'none',
    actions: [],
  };
}

describe('GameDetailsPage', () => {
  let target: HTMLDivElement;
  let component: object | undefined;
  let disposeInvoker: (() => void) | undefined;
  let invokedCommands: string[];
  let unexpectedCommands: string[];

  beforeEach(() => {
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
    const invoker = ((command: string) => {
      invokedCommands.push(command);
      if (command === 'renodx_availability') {
        return Promise.resolve(unsupportedRenoDxAvailability());
      }
      if (command === 'luma_availability') {
        return Promise.resolve(unsupportedLumaAvailability());
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
        return Promise.resolve(null);
      }
      if (command === 'list_game_executable_candidates') {
        return Promise.resolve([]);
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
      hasOtherTab: false,
      hasRenoDx: false,
      hasLuma: false,
    },
    {
      capabilities: ['renodx'] as const,
      hasOtherTab: true,
      hasRenoDx: true,
      hasLuma: false,
    },
    {
      capabilities: ['luma'] as const,
      hasOtherTab: true,
      hasRenoDx: false,
      hasLuma: true,
    },
    {
      capabilities: ['renodx', 'luma'] as const,
      hasOtherTab: true,
      hasRenoDx: true,
      hasLuma: true,
    },
  ])(
    'gates the add-on tab, cards, and availability for $capabilities',
    async ({ capabilities, hasOtherTab, hasRenoDx, hasLuma }) => {
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
      });

      const text = target.textContent;
      const tabLabels = [...target.querySelectorAll<HTMLElement>('[role="tab"]')].map((tab) =>
        tab.textContent.trim(),
      );
      expect(tabLabels.includes('Other')).toBe(hasOtherTab);
      expect(text.includes('RenoDX HDR')).toBe(hasRenoDx);
      expect(text.includes('Luma Framework')).toBe(hasLuma);
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
    expect(target.textContent).not.toContain('Other');

    host.replaceDetails(createGameDetails({ addon_capabilities: ['renodx'] }));
    flushSync();
    await vi.waitFor(() => {
      expect(invokedCommands.filter((command) => command === 'renodx_availability')).toHaveLength(
        2,
      );
    });

    expect(target.textContent).toContain('RenoDX HDR');
    expect(target.textContent).toContain('Other');
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
