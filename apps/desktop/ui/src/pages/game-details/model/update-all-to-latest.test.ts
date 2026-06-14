import { describe, expect, it } from 'vitest';

import { buildUpdateAllToLatestPlan } from './update-all-to-latest';
import { candidate, component, details, group } from './candidate-group-fixtures';

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
            candidate('3.7.0', { artifact_id: 'sr-370', is_downloaded: false }),
            candidate('3.6.0', { artifact_id: 'sr-360' }),
          ]),
        ],
      ),
    );

    expect(plan.updateCount).toBe(1);
    expect(plan.items[0]).toEqual({
      componentId: 'sr',
      artifactId: 'sr-370',
      isDownloaded: false,
    });
  });

  it('chooses the highest version even when candidates arrive out of order', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('sr', 'nvidia_dlss_sr')],
        [
          group('sr', 'nvidia_dlss_sr', '3.5.0', [
            candidate('3.6.0', { artifact_id: 'sr-360' }),
            candidate('3.10.0', { artifact_id: 'sr-3100' }),
            candidate('3.7.0', { artifact_id: 'sr-370' }),
          ]),
        ],
      ),
    );

    expect(plan.updateCount).toBe(1);
    expect(plan.items[0]?.artifactId).toBe('sr-3100');
  });

  it('skips components whose only candidates are not upgrades', () => {
    const plan = buildUpdateAllToLatestPlan(
      details(
        [component('sr', 'nvidia_dlss_sr')],
        [
          group('sr', 'nvidia_dlss_sr', '3.7.0', [
            candidate('3.6.0', { artifact_id: 'sr-360', comparison: 'older_version' }),
            candidate(null, { artifact_id: 'sr-unknown', comparison: 'unknown_version' }),
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
          group('sr', 'nvidia_dlss_sr', '3.5.0', [candidate('3.7.0', { artifact_id: 'sr-370' })]),
          group('sl-a', 'nvidia_streamline', '2.3.0', [
            candidate('2.4.0', { artifact_id: 'a-240' }),
            candidate('2.2.0', { artifact_id: 'a-220' }),
          ]),
          group('sl-b', 'nvidia_streamline', '2.3.0', [
            candidate('2.4.0', { artifact_id: 'b-240' }),
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
            candidate('2.5.0', { artifact_id: 'a-250' }),
            candidate('2.4.0', { artifact_id: 'a-240' }),
          ]),
          group('sl-b', 'nvidia_streamline', '2.3.0', [
            candidate('2.4.0', { artifact_id: 'b-240' }),
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
          group('sr', 'nvidia_dlss_sr', '3.5.0', [candidate('3.7.0', { artifact_id: 'sr-370' })]),
          // Only sl-a has any candidate → every Streamline version is incomplete.
          group('sl-a', 'nvidia_streamline', '2.3.0', [
            candidate('2.5.0', { artifact_id: 'a-250' }),
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
