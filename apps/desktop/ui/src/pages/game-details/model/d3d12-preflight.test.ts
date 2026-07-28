import { describe, expect, it } from 'vitest';

import {
  blockedD3d12Preflight,
  DEVELOPER_MODE_CHECK_UNAVAILABLE,
  DEVELOPER_MODE_REQUIRED,
  requiresD3d12Preflight,
} from './d3d12-preflight';

describe('D3D12 preflight selection', () => {
  it('preflights every D3D12 selection and leaves other technologies direct', () => {
    expect(requiresD3d12Preflight('d3d12_agility')).toBe(true);
    expect(requiresD3d12Preflight('vulkan')).toBe(false);
    expect(requiresD3d12Preflight('nvidia_dlss_sr')).toBe(false);
  });
});

describe('D3D12 preflight blocker classification', () => {
  it('offers recovery only when every blocker belongs to Developer Mode', () => {
    expect(blockedD3d12Preflight([DEVELOPER_MODE_REQUIRED])).toMatchObject({
      recovery: DEVELOPER_MODE_REQUIRED,
    });
    expect(
      blockedD3d12Preflight([DEVELOPER_MODE_REQUIRED, 'd3d12_executable_repair_required']),
    ).toMatchObject({
      recovery: null,
    });
  });

  it('gives an unavailable check precedence across a recoverable batch', () => {
    expect(
      blockedD3d12Preflight([
        DEVELOPER_MODE_REQUIRED,
        DEVELOPER_MODE_CHECK_UNAVAILABLE,
        DEVELOPER_MODE_REQUIRED,
      ]),
    ).toEqual({
      kind: 'blocked',
      blockers: [DEVELOPER_MODE_REQUIRED, DEVELOPER_MODE_CHECK_UNAVAILABLE],
      recovery: DEVELOPER_MODE_CHECK_UNAVAILABLE,
    });
  });

  it('deduplicates exact blockers without discarding malformed wire values', () => {
    expect(blockedD3d12Preflight(['', 'future_blocker', '', 'future_blocker'])).toEqual({
      kind: 'blocked',
      blockers: ['', 'future_blocker'],
      recovery: null,
    });
  });
});
