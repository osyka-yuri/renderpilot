import { describe, expect, it } from 'vitest';

import { buildUpdateAllToLatestPlan } from './update-all-to-latest';
import { candidate, catalogCandidate, component, details, group } from './candidate-group-fixtures';

describe('buildUpdateAllToLatestPlan', () => {
  it('returns an empty plan when there are no details', () => {
    expect(buildUpdateAllToLatestPlan(null)).toEqual({ items: [], updateCount: 0 });
  });

  it('picks the newest upgrade for an independent component', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('sr', 'nvidia_dlss_sr')],
        [
          group('sr', 'nvidia_dlss_sr', '3.5.0', [
            catalogCandidate('3.7.0', { artifact_id: 'sr-370', is_downloaded: false }),
            catalogCandidate('3.6.0', { artifact_id: 'sr-360' }),
          ]),
        ],
      ),
    );

    expect(plan.updateCount).toBe(1);
    expect(plan.items[0]).toEqual({
      componentId: 'sr',
      artifactId: 'sr-370',
      isDownloaded: false,
      d3d12ExecutableAction: null,
    });
  });

  it('chooses the highest version even when candidates arrive out of order', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('sr', 'nvidia_dlss_sr')],
        [
          group('sr', 'nvidia_dlss_sr', '3.5.0', [
            catalogCandidate('3.6.0', { artifact_id: 'sr-360' }),
            catalogCandidate('3.10.0', { artifact_id: 'sr-3100' }),
            catalogCandidate('3.7.0', { artifact_id: 'sr-370' }),
          ]),
        ],
      ),
    );

    expect(plan.updateCount).toBe(1);
    expect(plan.items[0]?.artifactId).toBe('sr-3100');
  });

  it('uses the full catalog release when technical PE versions are equal', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('dxc', 'microsoft_dxc')],
        [
          group('dxc', 'microsoft_dxc', '10.0.0', [
            catalogCandidate('10.1.0', {
              artifact_id: 'dxc-older-package',
              catalog_package: {
                package_id: 'dxc-older-package',
                release: {
                  version: '1.9.2602.16',
                  channel: 'stable',
                  label: null,
                },
                availability: 'available',
                automatic_selection_allowed: true,
              },
            }),
            catalogCandidate('10.1.0', {
              artifact_id: 'dxc-newer-package',
              catalog_package: {
                package_id: 'dxc-newer-package',
                release: {
                  version: '1.9.2602.17',
                  channel: 'stable',
                  label: null,
                },
                availability: 'available',
                automatic_selection_allowed: true,
              },
            }),
          ]),
        ],
      ),
    );

    expect(plan.items[0]?.artifactId).toBe('dxc-newer-package');
  });

  it('never selects preview or local-only candidates automatically', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('sr', 'nvidia_dlss_sr')],
        [
          group('sr', 'nvidia_dlss_sr', '3.5.0', [
            catalogCandidate('9.0.0', {
              artifact_id: 'preview',
              catalog_package: {
                package_id: 'preview',
                release: {
                  version: '9.0.0-preview',
                  channel: 'preview',
                  label: null,
                },
                availability: 'available',
                automatic_selection_allowed: false,
              },
            }),
            catalogCandidate('8.0.0', {
              artifact_id: 'local-only',
              catalog_package: {
                package_id: 'local-only',
                release: { version: '8.0.0', channel: 'stable', label: null },
                availability: 'local_only',
                automatic_selection_allowed: false,
              },
            }),
            catalogCandidate('3.7.0', { artifact_id: 'stable-active' }),
          ]),
        ],
      ),
    );

    expect(plan.items.map((item) => item.artifactId)).toEqual(['stable-active']);
  });

  it('skips components whose only candidates are not upgrades', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('sr', 'nvidia_dlss_sr')],
        [
          group('sr', 'nvidia_dlss_sr', '3.7.0', [
            catalogCandidate('3.6.0', {
              artifact_id: 'sr-360',
              comparison: 'older_version',
            }),
            candidate(null, { artifact_id: 'sr-unknown', comparison: 'unknown_version' }),
          ]),
        ],
      ),
    );

    expect(plan).toEqual({ items: [], updateCount: 0 });
  });

  it('excludes a D3D12 candidate while its executable requires repair', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('d3d12', 'd3d12_agility')],
        [
          group('d3d12', 'd3d12_agility', '1.606.4', [
            catalogCandidate('1.619.1', {
              artifact_id: 'd3d12-619',
              d3d12_executable_action: {
                kind: 'repair_required',
                executable_path: 'C:/Game/game.exe',
                backup_path: 'C:/Game/game.exe.bak',
                backup_exists: false,
                original_sdk_version: 606,
                current_sdk_version: 619,
                target_sdk_version: 619,
                requires_confirmation: false,
              },
            }),
          ]),
        ],
      ),
    );

    expect(plan).toEqual({ items: [], updateCount: 0 });
  });

  it('combines independent upgrades with the newest Streamline bundle version', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [
          component('sr', 'nvidia_dlss_sr'),
          component('sl-a', 'nvidia_streamline'),
          component('sl-b', 'nvidia_streamline'),
        ],
        [
          group('sr', 'nvidia_dlss_sr', '3.5.0', [
            catalogCandidate('3.7.0', { artifact_id: 'sr-370' }),
          ]),
          group('sl-a', 'nvidia_streamline', '2.3.0', [
            catalogCandidate('2.4.0', { artifact_id: 'a-240' }),
            catalogCandidate('2.2.0', { artifact_id: 'a-220' }),
          ]),
          group('sl-b', 'nvidia_streamline', '2.3.0', [
            catalogCandidate('2.4.0', { artifact_id: 'b-240' }),
          ]),
        ],
      ),
    );

    expect(plan.updateCount).toBe(3);
    expect(plan.items.map((item) => item.artifactId).sort()).toEqual(['a-240', 'b-240', 'sr-370']);
  });

  it('skips an incomplete newest Streamline version for an older complete one', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('sl-a', 'nvidia_streamline'), component('sl-b', 'nvidia_streamline')],
        [
          // Only sl-a can reach 2.5.0 → incomplete; both can reach 2.4.0 → complete.
          group('sl-a', 'nvidia_streamline', '2.3.0', [
            catalogCandidate('2.5.0', { artifact_id: 'a-250' }),
            catalogCandidate('2.4.0', { artifact_id: 'a-240' }),
          ]),
          group('sl-b', 'nvidia_streamline', '2.3.0', [
            catalogCandidate('2.4.0', { artifact_id: 'b-240' }),
          ]),
        ],
      ),
    );

    expect(plan.updateCount).toBe(2);
    expect(plan.items.map((item) => item.artifactId).sort()).toEqual(['a-240', 'b-240']);
  });

  it('skips Streamline entirely when no version every plugin can reach exists', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [
          component('sr', 'nvidia_dlss_sr'),
          component('sl-a', 'nvidia_streamline'),
          component('sl-b', 'nvidia_streamline'),
        ],
        [
          group('sr', 'nvidia_dlss_sr', '3.5.0', [
            catalogCandidate('3.7.0', { artifact_id: 'sr-370' }),
          ]),
          // Only sl-a has any candidate → every Streamline version is incomplete.
          group('sl-a', 'nvidia_streamline', '2.3.0', [
            catalogCandidate('2.5.0', { artifact_id: 'a-250' }),
          ]),
          group('sl-b', 'nvidia_streamline', '2.3.0', []),
        ],
      ),
    );

    expect(plan.updateCount).toBe(1);
    expect(plan.items[0]?.artifactId).toBe('sr-370');
  });

  it('reports nothing to update when everything is current', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('sr', 'nvidia_dlss_sr'), component('sl-a', 'nvidia_streamline')],
        [
          group('sr', 'nvidia_dlss_sr', '3.7.0', []),
          group('sl-a', 'nvidia_streamline', '2.4.0', []),
        ],
      ),
    );

    expect(plan).toEqual({ items: [], updateCount: 0 });
  });
});
