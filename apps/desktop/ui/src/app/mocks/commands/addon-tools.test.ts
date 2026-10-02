import { describe, expect, it } from 'vitest';

import { mockInvoker } from '../desktop';

describe('mock addon-tools IPC', () => {
  it('resolves luma_availability without throwing', async () => {
    const report = await mockInvoker('luma_availability', { gameId: 'steam:1' });
    expect(report).toMatchObject({
      state: { status: 'not_installed' },
      outcome: { kind: 'unsupported' },
    });
  });

  it('resolves renodx_availability without throwing', async () => {
    const report = await mockInvoker('renodx_availability', { gameId: 'steam:1' });
    expect(report).toMatchObject({
      state: { status: 'not_installed' },
      outcome: { kind: 'unsupported' },
      has_persisted_record: false,
      install_requires_shared_vulkan: false,
      vulkan_layer: { layer_detection: 'not_installed' },
    });
  });

  it('rejects luma write commands with an explicit mock message', async () => {
    await expect(
      mockInvoker('luma_install', { gameId: 'steam:1', gameContextToken: 'game-token' }),
    ).rejects.toThrow(/Mock preview does not simulate/);
  });

  it('resolves OptiScaler availability and rejects preview mutations explicitly', async () => {
    const report = await mockInvoker('get_optiscaler_availability', {
      gameId: 'steam:1',
    });
    expect(report).toMatchObject({
      game_id: 'steam:1',
      install: { installed: false },
      eligibility: { available: false },
    });
    await expect(
      mockInvoker('install_optiscaler', {
        gameId: 'steam:1',
        modules: ['core'],
        gameContextToken: 'game-token',
      }),
    ).rejects.toThrow(/Mock preview does not simulate/);
  });

  it('advertises all add-on manifests on refresh_remote_manifests', async () => {
    const report = await mockInvoker('refresh_remote_manifests', undefined);
    expect(report).toMatchObject({
      kinds: {
        luma: { status: 'ok' },
        renodx: { status: 'ok' },
        optiscaler: { status: 'ok' },
      },
    });
  });
});
