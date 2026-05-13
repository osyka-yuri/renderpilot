import { describe, expect, it } from 'vitest';
import {
  displayValue,
  buildComponentRows,
  buildConfiguredRow,
  buildLibrarySections,
  buildVendorBlocks,
  sameSelectionMap,
  reconcileArtifactSelections,
  reconcileNvapiSelections,
  selectionKey,
  installedOptionsForRow,
  candidateOptionsForRow,
} from './graphics-configurator';
import type { GameDetails } from '@entities/game';
import type { ComponentConfiguratorRow, ConfiguredComponentRow } from './graphics-configurator';
import type { GraphicsComponent, CandidateGroup, Candidate } from '@entities/component';

describe('graphics-configurator', () => {
  describe('displayValue', () => {
    it('returns value for non-empty string', () => {
      expect(displayValue('1.2.3')).toBe('1.2.3');
    });

    it('returns Unknown for null', () => {
      expect(displayValue(null)).toBe('Unknown');
    });

    it('returns Unknown for undefined', () => {
      expect(displayValue(undefined)).toBe('Unknown');
    });

    it('returns Unknown for empty string', () => {
      expect(displayValue('')).toBe('Unknown');
    });
  });

  describe('buildComponentRows', () => {
    it('builds rows from game details', () => {
      const details = createGameDetails({
        components: [createComponent({ id: 'comp-1', technology: 'dlss_super_resolution' })],
        candidate_groups: [
          createCandidateGroup({ component_id: 'comp-1', file_path: 'C:/game/dlss.dll' }),
        ],
      });

      const rows = buildComponentRows(details);

      expect(rows).toHaveLength(1);
      expect(rows[0].component.id).toBe('comp-1');
      expect(rows[0].group).not.toBeNull();
      expect(rows[0].installedOptions).toHaveLength(1);
    });

    it('uses null group when no candidate group matches', () => {
      const details = createGameDetails({
        components: [createComponent({ id: 'comp-1' })],
        candidate_groups: [],
      });

      const rows = buildComponentRows(details);

      expect(rows[0].group).toBeNull();
    });

    it('filters unknown technology components out of the visible configurator rows', () => {
      const details = createGameDetails({
        components: [
          createComponent({ id: 'comp-1', technology: 'dlss_super_resolution' }),
          createComponent({ id: 'comp-2', technology: 'Unknown' }),
        ],
        candidate_groups: [createCandidateGroup({ component_id: 'comp-2' })],
      });

      const rows = buildComponentRows(details);

      expect(rows).toHaveLength(1);
      expect(rows[0].component.id).toBe('comp-1');
    });
  });

  describe('buildConfiguredRow', () => {
    it('marks canBuildPlan false when no candidate is selected', () => {
      const row = createConfiguratorRow();
      const configured = buildConfiguredRow(row, {}, false);

      expect(configured.canBuildPlan).toBe(false);
    });

    it('marks canBuildPlan true when candidate is selected and not busy', () => {
      const row = createConfiguratorRow({
        group: createCandidateGroup({
          candidates: [createCandidate({ artifact_id: 'art-1' })],
        }),
      });
      const configured = buildConfiguredRow(row, { 'comp-1': 'art-1' }, false);

      expect(configured.canBuildPlan).toBe(true);
    });

    it('marks canBuildPlan false when busy', () => {
      const row = createConfiguratorRow({
        group: createCandidateGroup({
          candidates: [createCandidate({ artifact_id: 'art-1' })],
        }),
      });
      const configured = buildConfiguredRow(row, { 'comp-1': 'art-1' }, true);

      expect(configured.canBuildPlan).toBe(false);
    });
  });

  describe('buildLibrarySections', () => {
    it('groups rows by technology/library', () => {
      const rows = [
        createConfiguredRow({
          component: createComponent({ technology: 'dlss_super_resolution' }),
        }),
        createConfiguredRow({
          component: createComponent({ technology: 'dlss_super_resolution' }),
        }),
        createConfiguredRow({ component: createComponent({ technology: 'amd_fsr' }) }),
      ];

      const sections = buildLibrarySections(rows);

      expect(sections).toHaveLength(2);
    });

    it('uses compact labels for section headings', () => {
      const rows = [
        createConfiguredRow({
          component: createComponent({ technology: 'dlss_super_resolution' }),
        }),
        createConfiguredRow({
          component: createComponent({ id: 'comp-2', technology: 'amd_fsr_frame_generation' }),
        }),
      ];

      const sections = buildLibrarySections(rows);

      expect(sections.map((section) => section.label)).toEqual(['DLSS SR', 'FSR FG']);
    });
  });

  describe('buildVendorBlocks', () => {
    it('groups sections by vendor', () => {
      const rows = [
        createConfiguredRow({
          component: createComponent({ technology: 'dlss_super_resolution' }),
        }),
        createConfiguredRow({ component: createComponent({ technology: 'amd_fsr' }) }),
      ];
      const sections = buildLibrarySections(rows);
      const blocks = buildVendorBlocks(sections);

      const nvidiaBlock = blocks.find((b) => b.key === 'nvidia');
      const amdBlock = blocks.find((b) => b.key === 'amd');

      expect(nvidiaBlock).toBeDefined();
      expect(amdBlock).toBeDefined();
    });

    it('omits empty other vendor block', () => {
      const rows = [
        createConfiguredRow({
          component: createComponent({ technology: 'dlss_super_resolution' }),
        }),
      ];
      const sections = buildLibrarySections(rows);
      const blocks = buildVendorBlocks(sections);

      const otherBlock = blocks.find((b) => b.key === 'other');

      expect(otherBlock).toBeUndefined();
    });
  });

  describe('sameSelectionMap', () => {
    it('returns true for identical maps', () => {
      expect(sameSelectionMap({ a: '1' }, { a: '1' })).toBe(true);
    });

    it('returns false when keys differ', () => {
      expect(sameSelectionMap({ a: '1' }, { a: '1', b: '2' })).toBe(false);
    });

    it('returns false when values differ', () => {
      expect(sameSelectionMap({ a: '1' }, { a: '2' })).toBe(false);
    });
  });

  describe('reconcileArtifactSelections', () => {
    it('preserves current selection when candidate is still valid', () => {
      const row = createConfiguratorRow({
        group: createCandidateGroup({
          candidates: [createCandidate({ artifact_id: 'art-1' })],
        }),
      });
      const result = reconcileArtifactSelections([row], { 'comp-1': 'art-1' }, null);

      expect(result['comp-1']).toBe('art-1');
    });

    it('falls back to first candidate when current selection is invalid', () => {
      const row = createConfiguratorRow({
        group: createCandidateGroup({
          candidates: [
            createCandidate({ artifact_id: 'art-1' }),
            createCandidate({ artifact_id: 'art-2' }),
          ],
        }),
      });
      const result = reconcileArtifactSelections([row], { 'comp-1': 'stale' }, null);

      expect(result['comp-1']).toBe('art-1');
    });

    it('prefers active plan over current selection', () => {
      const row = createConfiguratorRow({
        group: createCandidateGroup({
          file_path: 'C:/game/dlss.dll',
          candidates: [createCandidate({ artifact_id: 'art-plan' })],
        }),
      });
      const plan = createSwapPlan({ target_path: 'C:/game/dlss.dll', artifact_id: 'art-plan' });
      const result = reconcileArtifactSelections([row], { 'comp-1': 'art-old' }, plan);

      expect(result['comp-1']).toBe('art-plan');
    });

    it('returns empty object when no candidates exist', () => {
      const row = createConfiguratorRow({ group: null });
      const result = reconcileArtifactSelections([row], { 'comp-1': 'art-1' }, null);

      expect(result).toEqual({});
    });
  });

  describe('reconcileNvapiSelections', () => {
    it('preserves valid current NVAPI selection', () => {
      const row = createConfiguratorRow({
        nvapiControls: [
          {
            id: 'preset',
            options: [{ value: 'quality', label: 'Quality' }],
            defaultValue: 'safe_original',
            label: 'Preset',
            description: '',
          },
        ],
      });
      const result = reconcileNvapiSelections([row], { 'comp-1::preset': 'quality' });

      expect(result['comp-1::preset']).toBe('quality');
    });

    it('falls back to default when current selection is invalid', () => {
      const row = createConfiguratorRow({
        nvapiControls: [
          {
            id: 'preset',
            options: [{ value: 'quality', label: 'Quality' }],
            defaultValue: 'safe_original',
            label: 'Preset',
            description: '',
          },
        ],
      });
      const result = reconcileNvapiSelections([row], { 'comp-1::preset': 'stale' });

      expect(result['comp-1::preset']).toBe('safe_original');
    });
  });

  describe('selectionKey', () => {
    it('joins component and control id with double colon', () => {
      expect(selectionKey('comp-1', 'preset')).toBe('comp-1::preset');
    });
  });

  describe('installedOptionsForRow', () => {
    it('returns value-label pairs from installed options', () => {
      const row = createConfiguredRow({
        installedOptions: [
          { value: 'path-a', label: 'A.dll', path: 'path-a', version: null, sha256: null },
        ],
      });
      const options = installedOptionsForRow(row);

      expect(options).toEqual([{ value: 'path-a', label: 'A.dll' }]);
    });
  });

  describe('candidateOptionsForRow', () => {
    it('returns disabled placeholder when no candidates exist', () => {
      const row = createConfiguredRow({ group: null });
      const options = candidateOptionsForRow(row);

      expect(options).toHaveLength(1);
      expect(options[0].disabled).toBe(true);
    });

    it('returns candidate options with version labels', () => {
      const row = createConfiguredRow({
        group: createCandidateGroup({
          candidates: [createCandidate({ artifact_id: 'art-1', version: '2.0.0' })],
        }),
      });
      const options = candidateOptionsForRow(row);

      expect(options).toHaveLength(1);
      expect(options[0].value).toBe('art-1');
    });
  });
});

// --- Test fixture factories ---

function createComponent(overrides: Partial<GraphicsComponent> = {}): GraphicsComponent {
  return {
    id: 'comp-1',
    game_id: 'game-1',
    kind: 'dll',
    technology: 'dlss_super_resolution',
    swappability: 'replaceable',
    files: [{ path: 'C:/game/dlss.dll', version: '1.0.0', sha256: null }],
    ...overrides,
  };
}

function createCandidateGroup(overrides: Partial<CandidateGroup> = {}): CandidateGroup {
  return {
    component_id: 'comp-1',
    technology: 'dlss_super_resolution',
    file_path: 'C:/game/dlss.dll',
    candidates: [createCandidate()],
    ...overrides,
  };
}

function createCandidate(overrides: Partial<Candidate> = {}): Candidate {
  return {
    artifact_id: 'art-1',
    file_name: 'dlss.dll',
    file_path: 'C:/repo/dlss.dll',
    comparison: 'newer',
    ...overrides,
  };
}

function createGameDetails(overrides: Partial<GameDetails> = {}): GameDetails {
  return {
    game: {
      identity: { id: 'game-1', title: 'Test Game', launcher: 'Steam', external_id: null },
      platform: 'Windows',
      runtime: 'Native',
      install_path: 'C:/game',
      executable_candidates: ['C:/game/game.exe'],
    },
    components: [],
    candidate_groups: [],
    operations: [],
    ...overrides,
  };
}

function createConfiguratorRow(
  overrides: Partial<ComponentConfiguratorRow> = {},
): ComponentConfiguratorRow {
  const component = overrides.component ?? createComponent();
  return {
    component,
    group: null,
    installedOptions: [
      {
        value: 'C:/game/dlss.dll',
        label: 'dlss.dll · v1.0.0',
        path: 'C:/game/dlss.dll',
        version: '1.0.0',
        sha256: null,
      },
    ],
    installedValue: 'C:/game/dlss.dll',
    nvapiControls: [],
    ...overrides,
  };
}

function createConfiguredRow(
  overrides: Partial<ConfiguredComponentRow> = {},
): ConfiguredComponentRow {
  const base = createConfiguratorRow(overrides);
  return {
    ...base,
    currentInstalled: base.installedOptions[0] ?? {
      value: 'missing:comp-1',
      label: 'No detected file',
      path: 'No file recorded',
      version: null,
      sha256: null,
    },
    selectedCandidate: null,
    candidatePath: 'No compatible local replacements were found for this component.',
    candidateSummary: 'This detected component has no local DLL replacement candidates yet.',
    canBuildPlan: false,
    ...overrides,
  };
}

function createSwapPlan(overrides: { target_path: string; artifact_id: string }) {
  return {
    operation_id: 'op-1',
    confirmation_token: 'token-1',
    game_id: 'game-1',
    operation_type: 'replace_component',
    target_path: overrides.target_path,
    replacement_path: 'C:/repo/dlss.dll',
    original_version: null,
    replacement_version: null,
    original_sha256: null,
    replacement_sha256: null,
    risk_level: 'low' as const,
    requires_backup: true,
    requires_elevation: false,
    artifact_id: overrides.artifact_id,
    blockers: [],
    warnings: [],
  };
}
