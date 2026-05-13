import { describe, expect, it } from 'vitest';
import type { GraphicsComponent, CandidateGroup, Candidate } from '@entities/component';
import type { GameDetails } from '@entities/game';
import type { SwapPlan } from '@entities/operation';
import { createGraphicsConfiguratorModel } from './create-graphics-configurator-model.svelte';

describe('createGraphicsConfiguratorModel', () => {
  it('starts empty when details are absent', () => {
    const model = createGraphicsConfiguratorModel({
      getDetails: () => null,
      getPlan: () => null,
      getBusy: () => false,
    });

    expect(model.viewModel).toBeNull();
    expect(model.selectedArtifacts).toEqual({});
    expect(model.selectedNvapiSelections).toEqual({});
  });

  it('reconciles artifact selection from the active plan', async () => {
    const model = createGraphicsConfiguratorModel({
      getDetails: () => createGameDetails(),
      getPlan: () => createSwapPlan({ artifact_id: 'art-2' }),
      getBusy: () => false,
    });

    await Promise.resolve();

    expect(model.selectedArtifacts['comp-1']).toBe('art-2');
  });

  it('updates artifact and nvapi selections through scenario handlers', () => {
    const model = createGraphicsConfiguratorModel({
      getDetails: () => createGameDetails(),
      getPlan: () => null,
      getBusy: () => false,
    });

    model.handleArtifactSelection('comp-1', 'art-2');
    model.handleNvapiSelection('comp-1', 'preset', 'quality');

    expect(model.selectedArtifacts['comp-1']).toBe('art-2');
    expect(model.selectedNvapiSelections['comp-1::preset']).toBe('quality');
  });
});

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
    candidates: [
      createCandidate({ artifact_id: 'art-1' }),
      createCandidate({ artifact_id: 'art-2' }),
    ],
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
    components: [
      createComponent({
        technology: 'dlss_super_resolution',
      }),
    ],
    candidate_groups: [createCandidateGroup()],
    operations: [],
    ...overrides,
  };
}

function createSwapPlan(overrides: Partial<SwapPlan> = {}): SwapPlan {
  return {
    operation_id: 'op-1',
    confirmation_token: 'confirm-1',
    game_id: 'game-1',
    operation_type: 'swap',
    target_path: 'C:/game/dlss.dll',
    replacement_path: 'C:/repo/dlss.dll',
    risk_level: 'safe',
    requires_backup: true,
    requires_elevation: false,
    artifact_id: 'art-1',
    blockers: [],
    warnings: [],
    ...overrides,
  };
}
