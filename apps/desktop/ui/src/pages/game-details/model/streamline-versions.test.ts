import { describe, expect, it } from 'vitest';

import type { CoordinatedCandidateOption } from '@entities/game';

import { buildStreamlineVersionModel } from './streamline-versions';
import { candidate, component, group } from './candidate-group-fixtures';

const STREAMLINE = 'nvidia_streamline';

function option(
  optionId: string,
  version: string,
  items: CoordinatedCandidateOption['items'],
): CoordinatedCandidateOption {
  return {
    option_id: optionId,
    release: { version, channel: 'stable', label: null },
    items,
  };
}

describe('buildStreamlineVersionModel', () => {
  it('uses backend-coordinated artifact ids instead of matching a display version', () => {
    const components = [component('a'), component('b')];
    const groupsById = {
      a: group('a', STREAMLINE, '2.3.0', [
        candidate('2.4.0', { artifact_id: 'a-decoy', is_downloaded: true }),
        candidate('2.4.0', { artifact_id: 'a-selected', is_downloaded: false }),
      ]),
      b: group('b', STREAMLINE, '2.3.0', [
        candidate('2.4.0', { artifact_id: 'b-decoy', is_downloaded: true }),
        candidate('2.4.0', { artifact_id: 'b-selected', is_downloaded: true }),
      ]),
    };

    const model = buildStreamlineVersionModel(components, groupsById, [
      option('b'.repeat(64), '2.4.0', [
        { component_id: 'a', artifact_id: 'a-selected' },
        { component_id: 'b', artifact_id: 'b-selected' },
      ]),
    ]);

    expect(model.options).toHaveLength(1);
    expect(model.options[0]).toMatchObject({
      optionId: 'b'.repeat(64),
      version: '2.4.0',
      allDownloaded: false,
    });
    expect(model.options[0].items.map((item) => item.artifactId)).toEqual([
      'a-selected',
      'b-selected',
    ]);
  });

  it('fails closed for malformed or stale coordinated items', () => {
    const components = [component('a'), component('b')];
    const groupsById = {
      a: group('a', STREAMLINE, '2.3.0', [candidate('2.4.0', { artifact_id: 'a-240' })]),
      b: group('b', STREAMLINE, '2.3.0', [candidate('2.4.0', { artifact_id: 'b-240' })]),
    };

    const model = buildStreamlineVersionModel(components, groupsById, [
      option('a'.repeat(64), '2.4.0', [
        { component_id: 'b', artifact_id: 'b-240' },
        { component_id: 'a', artifact_id: 'a-240' },
      ]),
      option('c'.repeat(64), '2.4.0', [
        { component_id: 'a', artifact_id: 'missing' },
        { component_id: 'b', artifact_id: 'b-240' },
      ]),
      option('d'.repeat(64), '2.4.0', [{ component_id: 'a', artifact_id: 'a-240' }]),
    ]);

    expect(model.options).toEqual([]);
  });

  it('sorts safe options by release while retaining option identity as the key', () => {
    const components = [component('a')];
    const groupsById = {
      a: group('a', STREAMLINE, '2.3.0', [
        candidate('2.5.0', { artifact_id: 'a-250' }),
        candidate('2.4.0', { artifact_id: 'a-240' }),
      ]),
    };

    const model = buildStreamlineVersionModel(components, groupsById, [
      option('b'.repeat(64), '2.4.0', [{ component_id: 'a', artifact_id: 'a-240' }]),
      option('a'.repeat(64), '2.5.0', [{ component_id: 'a', artifact_id: 'a-250' }]),
    ]);

    expect(model.options.map((entry) => [entry.version, entry.optionId])).toEqual([
      ['2.5.0', 'a'.repeat(64)],
      ['2.4.0', 'b'.repeat(64)],
    ]);
  });

  it('keeps backend version reports as the authority for mixed installed state', () => {
    const bundle = component('streamline');
    const groupsById = {
      streamline: {
        ...group('streamline', STREAMLINE, null, []),
        version_report: {
          kind: 'mixed' as const,
          min_technical_version: '2.4.0',
          max_technical_version: '2.9.0',
        },
      },
    };

    const model = buildStreamlineVersionModel([bundle], groupsById, []);
    expect(model.currentVersion).toBeNull();
    expect(model.isMixed).toBe(true);
    expect(model.versionRange).toEqual({ min: '2.4.0', max: '2.9.0' });
  });

  it('finds mixed version bounds in one pass regardless of component order', () => {
    const components = Object.freeze([
      component('newest'),
      component('oldest'),
      component('middle'),
    ]);
    const groupsById = {
      newest: group('newest', STREAMLINE, '2.10.0', []),
      oldest: group('oldest', STREAMLINE, '2.3.0', []),
      middle: group('middle', STREAMLINE, '2.7.0', []),
    };

    const model = buildStreamlineVersionModel(components, groupsById, []);

    expect(model.isMixed).toBe(true);
    expect(model.versionRange).toEqual({ min: '2.3.0', max: '2.10.0' });
    expect(components.map(({ id }) => id)).toEqual(['newest', 'oldest', 'middle']);
  });

  it('does not report a range for trailing-zero-equivalent versions', () => {
    const components = [component('short'), component('expanded')];
    const groupsById = {
      short: group('short', STREAMLINE, '2.4', []),
      expanded: group('expanded', STREAMLINE, '2.4.0', []),
    };

    const model = buildStreamlineVersionModel(components, groupsById, []);

    expect(model.currentVersion).toBe('2.4');
    expect(model.isMixed).toBe(false);
    expect(model.versionRange).toBeNull();
  });
});
