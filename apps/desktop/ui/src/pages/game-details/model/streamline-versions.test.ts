import { describe, expect, it } from 'vitest';
import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';

import { buildStreamlineVersionModel } from './streamline-versions';

type Candidate = GameCandidateGroup['candidates'][number];

function component(id: string): GameGraphicsComponent {
  return {
    id,
    game_id: 'game-1',
    kind: 'native_library',
    technology: 'nvidia_streamline',
    swappability: 'bundle_only',
    files: [{ path: `C:/Game/${id}.dll` }],
    rollback_available: false,
  };
}

function candidate(version: string, overrides: Partial<Candidate> = {}): Candidate {
  return {
    artifact_id: `artifact:${version}`,
    file_name: 'sl.plugin.dll',
    file_path: null,
    version,
    source_game_id: null,
    comparison: 'newer_version',
    warning: 'streamline_single_file_swap_requires_warning',
    manifest_entry_id: `entry:${version}`,
    is_downloaded: true,
    ...overrides,
  };
}

function group(
  componentId: string,
  current: string | null,
  candidates: Candidate[],
): GameCandidateGroup {
  return {
    component_id: componentId,
    technology: 'nvidia_streamline',
    file_path: `C:/Game/${componentId}.dll`,
    current_version: current,
    candidates,
  };
}

describe('buildStreamlineVersionModel', () => {
  it('lists versions newest-first and targets every plugin when aligned', () => {
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
    expect(model.options.map((option) => option.version)).toEqual(['2.4.0', '2.2.0']);

    const [v240] = model.options;
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

    const v240 = model.options.find((option) => option.version === '2.4.0');
    expect(v240?.updateCount).toBe(1);
    expect(v240?.items[0]?.componentId).toBe('b');
    expect(v240?.isComplete).toBe(true);
  });

  it('marks a version incomplete when a plugin cannot reach it', () => {
    const components = [component('a'), component('b')];
    const groupsById = {
      a: group('a', '2.3.0', [candidate('2.5.0', { artifact_id: 'a-250' })]),
      b: group('b', '2.3.0', []),
    };

    const model = buildStreamlineVersionModel(components, groupsById);

    const v250 = model.options.find((option) => option.version === '2.5.0');
    expect(v250?.updateCount).toBe(1);
    expect(v250?.missingCount).toBe(1);
    expect(v250?.isComplete).toBe(false);
  });

  it('flags allDownloaded=false and carries the manifest entry id', () => {
    const components = [component('a'), component('b')];
    const groupsById = {
      a: group('a', '2.3.0', [
        candidate('2.4.0', {
          artifact_id: 'a-240',
          is_downloaded: false,
          manifest_entry_id: 'e-a',
        }),
      ]),
      b: group('b', '2.3.0', [candidate('2.4.0', { artifact_id: 'b-240', is_downloaded: true })]),
    };

    const model = buildStreamlineVersionModel(components, groupsById);

    const v240 = model.options.find((option) => option.version === '2.4.0');
    expect(v240?.allDownloaded).toBe(false);
    expect(v240?.items.find((item) => item.componentId === 'a')?.entryId).toBe('e-a');
  });

  it('never offers the version a plugin is already on as a target', () => {
    const components = [component('a'), component('b')];
    const groupsById = {
      a: group('a', '2.4.0', [candidate('2.3.0', { artifact_id: 'a-230' })]),
      b: group('b', '2.4.0', [candidate('2.3.0', { artifact_id: 'b-230' })]),
    };

    const model = buildStreamlineVersionModel(components, groupsById);

    expect(model.currentVersion).toBe('2.4.0');
    expect(model.options.map((option) => option.version)).toEqual(['2.3.0']);
  });
});
