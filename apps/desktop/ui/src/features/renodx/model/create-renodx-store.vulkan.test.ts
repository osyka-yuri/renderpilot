import { describe, expect, it, vi } from 'vitest';

import type * as EntitiesLibrary from '@entities/library';

vi.mock('@shared/notifications', () => ({
  publishErrorNotification: vi.fn(),
}));

vi.mock('@entities/library', async (importOriginal) => {
  const actual = await importOriginal<typeof EntitiesLibrary>();
  return { ...actual, clearDownloadProgress: vi.fn() };
});

import { createRenoDxStore } from './create-renodx-store.svelte';
import type { AvailabilityReport } from './types';
import {
  availability,
  fakeApi,
  NOT_INSTALLED_SAFE,
  VULKAN_EXTERNAL_READ_ONLY,
  VULKAN_INSTALLED,
} from './renodx-store-test-fixtures';

describe('createRenoDxStore', () => {
  it('refreshes the Vulkan layer report after a transparent Vulkan install', async () => {
    const VULKAN: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'installable',
        confidence: 'untested',
        risk: {
          severity: 'info',
          message_key: 'addon.risk.sp_safe',
        },
        notes_keys: [],
        host_kind: 'vulkan',
      },
      manual_install: null,
    });
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(VULKAN)),
      vulkanLayerStatus: vi.fn(() => Promise.resolve(VULKAN_INSTALLED)),
    });
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');

    // Vulkan layer is installed transparently — no consent needed.
    await store.install('steam:1091500', 'nightly', false);

    expect(api.install).toHaveBeenCalledWith('steam:1091500', 'nightly', false);
    expect(api.vulkanLayerStatus).toHaveBeenCalled();
    // The layer report comes from the backend, not from optimistic inference.
    expect(store.vulkanLayer?.layer_detection).toBe('installed');
  });

  it('reuses an existing Vulkan layer without consent', async () => {
    const VULKAN_LAYER_PRESENT: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      outcome: {
        ...NOT_INSTALLED_SAFE.outcome,
        host_kind: 'vulkan',
      } as AvailabilityReport['outcome'],
      vulkan_layer: VULKAN_EXTERNAL_READ_ONLY,
    };
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(VULKAN_LAYER_PRESENT)) }),
    });
    await store.load('steam:1091500');
    expect(store.vulkanLayer?.actions.update).toBeUndefined();
    expect(store.vulkanLayer?.actions.switch_channel).toBeUndefined();
    expect(store.vulkanLayer?.actions.remove).toBeUndefined();
  });

  it('keeps Vulkan action permissions backend-authored', async () => {
    const VULKAN_LAYER_PRESENT: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      outcome: {
        ...NOT_INSTALLED_SAFE.outcome,
        host_kind: 'vulkan',
      } as AvailabilityReport['outcome'],
      vulkan_layer: VULKAN_INSTALLED,
    };
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(VULKAN_LAYER_PRESENT)) }),
    });

    await store.load('steam:1091500');

    expect(store.vulkanLayer?.layer_detection).toBe('installed');
    expect(store.vulkanLayer?.actions.update?.requires_confirmation).toBe(true);
    expect(store.vulkanLayer?.actions.update?.confirmation_scope).toBe('all_vulkan_reno_dx_games');
  });
});
