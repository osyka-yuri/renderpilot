import { describe, expect, it } from 'vitest';

import {
  d3d12ExecutableActionIdentity,
  uniqueD3d12ExecutableMutationActions,
  type D3d12ExecutableMutationAction,
} from './d3d12-executable-action';

const BASE_ACTION: D3d12ExecutableMutationAction = {
  kind: 'patch',
  executable_path: 'C:/Game/game.exe',
  backup_path: 'C:/Game/game.exe.bak',
  backup_exists: false,
  original_sdk_version: 606,
  current_sdk_version: 606,
  target_sdk_version: 619,
  requires_confirmation: true,
};

describe('D3D12 executable action identity', () => {
  it.each([
    ['kind', { kind: 'restore' }],
    ['executable_path', { executable_path: 'D:/Game/game.exe' }],
    ['backup_path', { backup_path: 'C:/Game/backup.exe' }],
    ['backup_exists', { backup_exists: true }],
    ['original_sdk_version', { original_sdk_version: 607 }],
    ['current_sdk_version', { current_sdk_version: 607 }],
    ['target_sdk_version', { target_sdk_version: 620 }],
  ] as const)('includes %s in the identity', (_field, change) => {
    expect(d3d12ExecutableActionIdentity({ ...BASE_ACTION, ...change })).not.toBe(
      d3d12ExecutableActionIdentity(BASE_ACTION),
    );
  });

  it('deduplicates identical actions while preserving their first occurrence order', () => {
    const otherAction = { ...BASE_ACTION, executable_path: 'D:/Game/other.exe' };

    expect(
      uniqueD3d12ExecutableMutationActions([
        BASE_ACTION,
        { ...BASE_ACTION },
        otherAction,
        { ...otherAction },
      ]),
    ).toEqual([BASE_ACTION, otherAction]);
  });

  it('shows one executable change when only backend token requirements differ', () => {
    const actionWithoutToken = { ...BASE_ACTION, requires_confirmation: false };

    expect(d3d12ExecutableActionIdentity(actionWithoutToken)).toBe(
      d3d12ExecutableActionIdentity(BASE_ACTION),
    );
    expect(uniqueD3d12ExecutableMutationActions([BASE_ACTION, actionWithoutToken])).toEqual([
      BASE_ACTION,
    ]);
  });
});
