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
      vulkan_layer: { layer_detection: 'not_installed' },
    });
  });

  it('rejects luma write commands with an explicit mock message', async () => {
    await expect(
      mockInvoker('luma_install', { gameId: 'steam:1', confirmAnticheat: false }),
    ).rejects.toThrow(/Mock preview does not simulate/);
  });

  it('still advertises luma and renodx kinds on refresh_remote_manifests', async () => {
    const report = await mockInvoker('refresh_remote_manifests', undefined);
    expect(report).toMatchObject({
      kinds: {
        luma: { status: 'ok' },
        renodx: { status: 'ok' },
      },
    });
  });
});
