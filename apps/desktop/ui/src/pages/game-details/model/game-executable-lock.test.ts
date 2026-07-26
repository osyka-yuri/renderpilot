import { describe, expect, it } from 'vitest';

import type { D3d12ExecutableStatus } from '@entities/game';

import { resolveExecutableLockReason } from './game-executable-lock';

describe('resolveExecutableLockReason', () => {
  it('returns null without an authoritative selection lock', () => {
    expect(resolveExecutableLockReason([])).toBeNull();
    expect(resolveExecutableLockReason([component(null)])).toBeNull();
    expect(
      resolveExecutableLockReason([
        component(status({ status: 'repair_required', selection_locked: false })),
      ]),
    ).toBeNull();
  });

  it('returns the managed reason for a locked non-repair state', () => {
    expect(resolveExecutableLockReason([component(status())])).toBe('d3d12_managed');
    expect(
      resolveExecutableLockReason([
        component(status({ status: 'original', selection_locked: true })),
      ]),
    ).toBe('d3d12_managed');
  });

  it('returns the repair reason for a locked repair state', () => {
    expect(
      resolveExecutableLockReason([
        component(status({ status: 'repair_required', selection_locked: true })),
      ]),
    ).toBe('d3d12_repair_required');
  });

  it('prioritizes repair guidance regardless of component order', () => {
    const managed = component(status());
    const repair = component(status({ status: 'repair_required', selection_locked: true }));

    expect(resolveExecutableLockReason([managed, repair])).toBe('d3d12_repair_required');
    expect(resolveExecutableLockReason([repair, managed])).toBe('d3d12_repair_required');
  });
});

function component(d3d12_executable_status: D3d12ExecutableStatus | null) {
  return { d3d12_executable_status };
}

function status(overrides: Partial<D3d12ExecutableStatus> = {}): D3d12ExecutableStatus {
  return {
    status: 'patched',
    selection_locked: true,
    executable_path: 'C:/Games/Test/game.exe',
    backup_path: 'C:/Games/Test/game.exe.bak',
    backup_exists: true,
    original_sdk_version: 606,
    current_sdk_version: 619,
    ...overrides,
  };
}
