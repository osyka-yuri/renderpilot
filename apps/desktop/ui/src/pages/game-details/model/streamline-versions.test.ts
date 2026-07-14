import { describe, expect, it } from 'vitest';

import { buildStreamlineVersionModel } from './streamline-versions';
import { compareVersionAsc, versionsEqual } from './version-compare';
import { candidate, component, group as makeGroup } from './candidate-group-fixtures';

const STREAMLINE = 'nvidia_streamline';

function group(
  componentId: string,
  current: string | null,
  candidates: Parameters<typeof makeGroup>[3],
) {
  return makeGroup(componentId, STREAMLINE, current, candidates);
}

function findOption(model: ReturnType<typeof buildStreamlineVersionModel>, version: string) {
  const option = model.options.find(
    (o) => o.version === version || versionsEqual(o.version, version),
  );
  if (!option) {
    throw new Error(`expected an option for version ${version}`);
  }
  return option;
}

describe('buildStreamlineVersionModel', () => {
  it('lists versions newest-first and includes the current version', () => {
    const components = [component('a'), component('b')];
    const groupsById = {
      a: group('a', '2.3.0', [
        candidate('2.4.0', { artifact_id: 'a-240' }),
        candidate('2.2.0', { artifact_id: 'a-220' }),
      ]),
      b: group('b', '2.3.0', [
        candidate('2.4.0', { artifact_id: 'b-240' }),
        candidate('2.2.0', { artifact_id: 'b-220' }),
      ]),
    };

    const model = buildStreamlineVersionModel(components, groupsById);

    expect(model.currentVersion).toBe('2.3.0');
    expect(model.isMixed).toBe(false);
    expect(model.totalCount).toBe(2);

    // Current version is always present so its SelectItem is never remounted.
    expect(model.options.map((o) => o.version)).toEqual(['2.4.0', '2.3.0', '2.2.0']);

    const current = findOption(model, '2.3.0');
    expect(current.isCurrent).toBe(true);
    expect(current.updateCount).toBe(0);
    expect(current.items).toEqual([]);

    const v240 = findOption(model, '2.4.0');
    expect(v240.isCurrent).toBe(false);
    expect(v240.label).toBe('v2.4.0');
    expect(v240.updateCount).toBe(2);
    expect(v240.isComplete).toBe(true);
    expect(v240.allDownloaded).toBe(true);
    expect(v240.items.map((item) => item.artifactId).sort()).toEqual(['a-240', 'b-240']);
  });

  it('reports mixed current versions and updates only the lagging plugin', () => {
    const components = [component('a'), component('b')];
    const groupsById = {
      a: group('a', '2.4.0', [candidate('2.2.0', { artifact_id: 'a-220' })]),
      b: group('b', '2.3.0', [
        candidate('2.4.0', { artifact_id: 'b-240' }),
        candidate('2.2.0', { artifact_id: 'b-220' }),
      ]),
    };

    const model = buildStreamlineVersionModel(components, groupsById);

    expect(model.currentVersion).toBeNull();
    expect(model.isMixed).toBe(true);
    expect(model.versionRange).toEqual({ min: '2.3.0', max: '2.4.0' });

    // When mixed, no single current version is known — nothing is pre-inserted.
    expect(model.options.every((o) => !o.isCurrent)).toBe(true);

    const v240 = findOption(model, '2.4.0');
    expect(v240.updateCount).toBe(1);
    expect(v240.items[0]?.componentId).toBe('b');
    expect(v240.isComplete).toBe(true);
  });

  it('marks a version incomplete when a plugin cannot reach it', () => {
    const components = [component('a'), component('b')];
    const groupsById = {
      a: group('a', '2.3.0', [candidate('2.5.0', { artifact_id: 'a-250' })]),
      b: group('b', '2.3.0', []),
    };

    const model = buildStreamlineVersionModel(components, groupsById);

    const v250 = findOption(model, '2.5.0');
    expect(v250.isCurrent).toBe(false);
    expect(v250.updateCount).toBe(1);
    expect(v250.missingCount).toBe(1);
    expect(v250.isComplete).toBe(false);
  });

  it('flags allDownloaded=false and carries each item download state', () => {
    const components = [component('a'), component('b')];
    const groupsById = {
      a: group('a', '2.3.0', [candidate('2.4.0', { artifact_id: 'a-240', is_downloaded: false })]),
      b: group('b', '2.3.0', [candidate('2.4.0', { artifact_id: 'b-240', is_downloaded: true })]),
    };

    const model = buildStreamlineVersionModel(components, groupsById);

    const v240 = findOption(model, '2.4.0');
    expect(v240.allDownloaded).toBe(false);
    expect(v240.items.find((item) => item.componentId === 'a')?.isDownloaded).toBe(false);
    expect(v240.items.find((item) => item.componentId === 'b')?.isDownloaded).toBe(true);
  });

  it('includes the current version alongside older candidates', () => {
    const components = [component('a'), component('b')];
    const groupsById = {
      a: group('a', '2.4.0', [candidate('2.3.0', { artifact_id: 'a-230' })]),
      b: group('b', '2.4.0', [candidate('2.3.0', { artifact_id: 'b-230' })]),
    };

    const model = buildStreamlineVersionModel(components, groupsById);

    expect(model.currentVersion).toBe('2.4.0');

    // Current version is in the list so the Select always has a stable item for it.
    expect(model.options.map((o) => o.version)).toEqual(['2.4.0', '2.3.0']);

    const current = findOption(model, '2.4.0');
    expect(current.isCurrent).toBe(true);
    expect(current.updateCount).toBe(0);
    expect(current.allDownloaded).toBe(true);

    const v230 = findOption(model, '2.3.0');
    expect(v230.isCurrent).toBe(false);
  });

  it('uses the backend mixed report without re-reading raw component files', () => {
    const bundle = component('streamline');
    const groupsById = {
      streamline: {
        ...group('streamline', null, [
          candidate('2.9.0', { artifact_id: 'pkg-290', file_name: 'sl.common.dll' }),
          candidate('2.4.0', { artifact_id: 'pkg-240', file_name: 'sl.common.dll' }),
        ]),
        version_report: { kind: 'mixed' as const, min_version: '2.4.0', max_version: '2.9.0' },
      },
    };

    const model = buildStreamlineVersionModel([bundle], groupsById);

    expect(model.isMixed).toBe(true);
    expect(model.currentVersion).toBeNull();
    expect(model.versionRange).toEqual({ min: '2.4.0', max: '2.9.0' });

    const v290 = findOption(model, '2.9.0');
    expect(v290.isCurrent).toBe(false);
    expect(v290.updateCount).toBe(1);
    expect(v290.items[0]?.artifactId).toBe('pkg-290');
  });

  it('treats trailing-zero-equivalent known reports as current', () => {
    const bundle = component('streamline');
    // PE often reports 2.9.0.0 while package options use 2.9.0 — equality must
    // mark the option current without inventing a second Select entry.
    const groupsById = {
      streamline: group('streamline', '2.9.0.0', [
        candidate('2.9.0', { artifact_id: 'pkg-290', file_name: 'sl.common.dll' }),
        candidate('2.8.0', { artifact_id: 'pkg-280', file_name: 'sl.common.dll' }),
      ]),
    };

    const model = buildStreamlineVersionModel([bundle], groupsById);

    expect(model.isMixed).toBe(false);
    expect(model.currentVersion).toBe('2.9.0.0');
    // First spelling of an equivalent release is kept (known report before package label).
    expect(model.options.map((o) => o.version)).toEqual(['2.9.0.0', '2.8.0']);
    const current = findOption(model, '2.9.0');
    expect(current.isCurrent).toBe(true);
    expect(current.version).toBe('2.9.0.0');
    expect(current.updateCount).toBe(0);
  });

  it('does not invent a uniform current from an unknown report', () => {
    const bundle = component('streamline');
    const groupsById = {
      streamline: group('streamline', null, [
        candidate('2.9.0', { artifact_id: 'pkg-290', file_name: 'sl.common.dll' }),
      ]),
    };

    const model = buildStreamlineVersionModel([bundle], groupsById);

    expect(model.currentVersion).toBeNull();
    expect(model.isMixed).toBe(false);
    const v290 = findOption(model, '2.9.0');
    expect(v290.isCurrent).toBe(false);
    expect(v290.updateCount).toBe(1);
  });
});

describe('versionsEqual (version-compare)', () => {
  it('ignores trailing zero segments', () => {
    expect(versionsEqual('2.9.0', '2.9.0.0')).toBe(true);
    expect(versionsEqual('2.9', '2.9.0.0')).toBe(true);
    expect(versionsEqual('2.9.0', '2.9.1')).toBe(false);
  });

  it('compares u64-sized segments without JavaScript Number rounding', () => {
    expect(compareVersionAsc('18446744073709551614', '18446744073709551615')).toBeLessThan(0);
    expect(compareVersionAsc('18446744073709551615.0', '18446744073709551615')).toBe(0);
  });
});
