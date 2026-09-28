import { describe, expect, it } from 'vitest';

import { buildUpdateAllToLatestPlan, resolveAutomaticCandidate } from './update-all-to-latest';
import { d3d12PlanFingerprint } from './swap-request';
import { catalogCandidate, component, details, group } from './candidate-group-fixtures';

describe('buildUpdateAllToLatestPlan', () => {
  it('returns an empty plan when there are no details', () => {
    expect(buildUpdateAllToLatestPlan(null)).toEqual({ items: [], updateCount: 0 });
  });

  it('resolves the exact backend-selected artifact without reapplying policy', () => {
    const selected = catalogCandidate('3.7.0', {
      artifact_id: 'selected',
      comparison: 'older_version',
      is_downloaded: false,
      catalog_package: {
        package_id: 'manual-looking',
        release: { version: '3.7.0', channel: 'preview', label: null },
        availability: 'local_only',
        automatic_selection_allowed: false,
        presentation: null,
      },
    });
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('sr', 'nvidia_dlss_sr')],
        [
          group(
            'sr',
            'nvidia_dlss_sr',
            '3.5.0',
            [catalogCandidate('9.0.0', { artifact_id: 'other' }), selected],
            'selected',
          ),
        ],
      ),
    );

    expect(plan).toEqual({
      items: [
        {
          kind: 'direct',
          target: {
            componentId: 'sr',
            artifactId: 'selected',
            isDownloaded: false,
          },
        },
      ],
      updateCount: 1,
    });
  });

  it('does not infer a selection when the backend id is absent', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('sr', 'nvidia_dlss_sr')],
        [
          group('sr', 'nvidia_dlss_sr', '3.5.0', [
            catalogCandidate('3.7.0', { artifact_id: 'eligible-looking' }),
          ]),
        ],
      ),
    );

    expect(plan).toEqual({ items: [], updateCount: 0 });
  });

  it('fails closed for dangling or duplicate selected artifact ids', () => {
    const dangling = group(
      'dangling',
      'nvidia_dlss_sr',
      '3.5.0',
      [catalogCandidate('3.7.0', { artifact_id: 'present' })],
      'missing',
    );
    const duplicated = group(
      'duplicated',
      'nvidia_dlss_sr',
      '3.5.0',
      [
        catalogCandidate('3.7.0', { artifact_id: 'same' }),
        catalogCandidate('3.8.0', { artifact_id: 'same' }),
      ],
      'same',
    );

    expect(resolveAutomaticCandidate(dangling)).toBeNull();
    expect(resolveAutomaticCandidate(duplicated)).toBeNull();
    expect(
      buildUpdateAllToLatestPlan(
        details(
          [component('dangling', 'nvidia_dlss_sr'), component('duplicated', 'nvidia_dlss_sr')],
          [dangling, duplicated],
        ),
      ),
    ).toEqual({ items: [], updateCount: 0 });
  });

  it('fails closed when multiple groups claim the same component id', () => {
    const first = group(
      'sr',
      'nvidia_dlss_sr',
      '3.5.0',
      [catalogCandidate('3.7.0', { artifact_id: 'first' })],
      'first',
    );
    const second = group(
      'sr',
      'nvidia_dlss_sr',
      '3.5.0',
      [catalogCandidate('3.8.0', { artifact_id: 'second' })],
      'second',
    );

    expect(
      buildUpdateAllToLatestPlan(details([component('sr', 'nvidia_dlss_sr')], [first, second])),
    ).toEqual({ items: [], updateCount: 0 });
  });

  it('keeps D3D12 preflight and component ordering in the resulting plan', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('d3d12', 'd3d12_agility'), component('sr', 'nvidia_dlss_sr')],
        [
          group(
            'sr',
            'nvidia_dlss_sr',
            '3.5.0',
            [catalogCandidate('3.7.0', { artifact_id: 'sr-selected' })],
            'sr-selected',
          ),
          group(
            'd3d12',
            'd3d12_agility',
            '1.606.4',
            [catalogCandidate('1.619.1', { artifact_id: 'd3d12-selected' })],
            'd3d12-selected',
          ),
        ],
      ),
    );

    expect(plan.items.map((item) => [item.kind, item.target.artifactId])).toEqual([
      ['d3d12', 'd3d12-selected'],
      ['direct', 'sr-selected'],
    ]);
  });

  it('binds its D3D12 item to the current component status and candidate EXE action', () => {
    const status = {
      status: 'patched' as const,
      selection_locked: false,
      executable_path: 'C:/Game/game.exe',
      backup_path: 'C:/Game/game.exe.bak',
      backup_exists: true,
      original_sdk_version: 606,
      current_sdk_version: 619,
    };
    const action = {
      kind: 'patch' as const,
      executable_path: status.executable_path,
      backup_path: status.backup_path,
      backup_exists: true,
      original_sdk_version: 606,
      current_sdk_version: 619,
      target_sdk_version: 620,
      requires_confirmation: false,
    };
    const d3dComponent = {
      ...component('d3d12', 'd3d12_agility'),
      d3d12_executable_status: status,
    };
    const candidate = catalogCandidate('1.620.1', {
      artifact_id: 'd3d12-selected',
      d3d12_executable_action: action,
    });
    const selectedGroup = group('d3d12', 'd3d12_agility', '1.619.1', [candidate], 'd3d12-selected');

    const firstPlan = buildUpdateAllToLatestPlan(details([d3dComponent], [selectedGroup]));
    const firstItem = firstPlan.items[0];
    expect(firstItem.kind).toBe('d3d12');
    if (firstItem.kind !== 'd3d12') {
      throw new Error('Expected a D3D12 planned swap');
    }
    expect(firstItem.planFingerprint).toBe(d3d12PlanFingerprint(status, action));

    const changedAction = { ...action, requires_confirmation: true };
    const secondPlan = buildUpdateAllToLatestPlan(
      details(
        [d3dComponent],
        [
          group(
            'd3d12',
            'd3d12_agility',
            '1.619.1',
            [{ ...candidate, d3d12_executable_action: changedAction }],
            'd3d12-selected',
          ),
        ],
      ),
    );
    expect(secondPlan.items[0]).toMatchObject({
      kind: 'd3d12',
      target: { componentId: 'd3d12', artifactId: 'd3d12-selected', isDownloaded: true },
    });
    const secondItem = secondPlan.items[0];
    if (secondItem.kind !== 'd3d12') {
      throw new Error('Expected a D3D12 planned swap');
    }
    expect(secondItem.planFingerprint).not.toBe(firstItem.planFingerprint);

    const changedStatus = { ...status, current_sdk_version: 620 };
    const thirdPlan = buildUpdateAllToLatestPlan(
      details([{ ...d3dComponent, d3d12_executable_status: changedStatus }], [selectedGroup]),
    );
    const thirdItem = thirdPlan.items[0];
    if (thirdItem.kind !== 'd3d12') {
      throw new Error('Expected a D3D12 planned swap');
    }
    expect(thirdItem.planFingerprint).not.toBe(firstItem.planFingerprint);
  });
});
