import { describe, expect, it } from 'vitest';

import { buildUpdateAllToLatestPlan, resolveAutomaticCandidate } from './update-all-to-latest';
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
});
