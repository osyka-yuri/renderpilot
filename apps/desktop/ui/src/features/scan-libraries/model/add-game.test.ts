import { describe, expect, it } from 'vitest';

import type { AddGameDecision, AddGameInspection } from './add-game';
import { automaticAddGameConfirmation, decisionOptions } from './add-game';

describe('backend-owned add-game decisions', () => {
  it('turns an automatic decision into a minimal confirmation', () => {
    const inspected = inspection({
      decision: {
        kind: 'automatic',
        option: { rootChoice: 'recommended', catalogAction: 'add' },
      },
    });

    if (inspected.decision.kind !== 'automatic') {
      throw new Error('test fixture must contain an automatic decision');
    }
    expect(automaticAddGameConfirmation(inspected.decision)).toEqual({
      rootChoice: 'recommended',
      allowRootCorrection: false,
      chosenExecutable: null,
    });
  });

  it('exposes exactly the backend-provided review options', () => {
    const decision: AddGameDecision = {
      kind: 'review',
      defaultOption: { rootChoice: 'recommended', catalogAction: 'add' },
      options: [
        { rootChoice: 'recommended', catalogAction: 'add' },
        { rootChoice: 'selected', catalogAction: 'correct_existing_root' },
      ],
    };

    expect(decisionOptions(decision)).toEqual(decision.options);
  });

  it('does not synthesize options for an unavailable decision', () => {
    expect(
      decisionOptions({
        kind: 'unavailable',
        reasons: ['multiple_installs', 'contains_multiple_catalog_installs'],
      }),
    ).toEqual([]);
  });
});

function inspection(overrides: Partial<AddGameInspection> = {}): AddGameInspection {
  return {
    selectedRoot: 'C:/Games/Black Flag',
    inspectionFingerprint: 'inspection:v1:test',
    catalogGeneration: 7,
    boundary: {
      kind: 'single_install',
      completeness: 'complete',
      candidateRoots: ['C:/Games/Black Flag'],
      evidence: ['root_executable'],
    },
    recommendation: null,
    relationship: {
      kind: 'new',
      gameIds: [],
      provenInstallRoots: [],
    },
    executables: [],
    requiresExplicitExecutable: false,
    rootCorrection: null,
    decision: {
      kind: 'automatic',
      option: { rootChoice: 'selected', catalogAction: 'add' },
    },
    warnings: [],
    ...overrides,
  };
}
