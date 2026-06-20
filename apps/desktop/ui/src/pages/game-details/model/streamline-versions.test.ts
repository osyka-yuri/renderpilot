import { describe, expect, it } from 'vitest';

import { buildStreamlineVersionModel } from './streamline-versions';
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
  const option = model.options.find((o) => o.version === version);
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
});
