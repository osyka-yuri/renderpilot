import { describe, expect, it } from 'vitest';

import { candidate } from './candidate-group-fixtures';
import { partitionD3d12Candidates } from './candidate-partition';

const action = (kind: 'none' | 'patch' | 'restore' | 'repair_required') => ({
  kind,
  executable_path: 'C:/Game/game.exe',
  backup_path: 'C:/Game/game.exe.renderpilot.bak',
  backup_exists: true,
  original_sdk_version: 606,
  current_sdk_version: 606,
  target_sdk_version: 619,
  requires_confirmation: kind === 'patch' || kind === 'restore',
});

describe('partitionD3d12Candidates', () => {
  it('keeps every candidate visible when the group contains EXE actions', () => {
    const withoutAction = candidate('1.0', {
      artifact_id: 'without-action',
      d3d12_executable_action: null,
    });
    const unchanged = candidate('1.1', {
      artifact_id: 'none',
      d3d12_executable_action: action('none'),
    });
    const patch = candidate('1.2', {
      artifact_id: 'patch',
      d3d12_executable_action: action('patch'),
    });
    const restore = candidate('1.3', {
      artifact_id: 'restore',
      d3d12_executable_action: action('restore'),
    });
    const repair = candidate('1.4', {
      artifact_id: 'repair',
      d3d12_executable_action: action('repair_required'),
    });

    const partition = partitionD3d12Candidates([withoutAction, unchanged, patch, restore, repair]);

    expect(partition.hasExecutableActions).toBe(true);
    expect(partition.compatible.map((item) => item.artifact_id)).toEqual([
      'without-action',
      'none',
    ]);
    expect(partition.changesExecutable.map((item) => item.artifact_id)).toEqual([
      'patch',
      'restore',
    ]);
    expect(partition.unavailable.map((item) => item.artifact_id)).toEqual(['repair']);
  });

  it('treats a legacy group without actions as compatible', () => {
    const candidates = [candidate('1.0'), candidate('1.1')];

    expect(partitionD3d12Candidates(candidates)).toEqual({
      hasExecutableActions: false,
      compatible: candidates,
      changesExecutable: [],
      unavailable: [],
    });
  });
});
