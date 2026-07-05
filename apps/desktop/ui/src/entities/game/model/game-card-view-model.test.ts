import { describe, expect, it } from 'vitest';
import type { GameSummary } from './types';
import { createGameSummary } from './test-support';
import { toGameCardViewModel } from './game-card-view-model';

function makeGameSummary(overrides: Partial<GameSummary> = {}): GameSummary {
  return createGameSummary(overrides); // delegate to shared helper
}

describe('game-card-view-model', () => {
  it('keeps raw library tags for UI-level formatting', () => {
    const viewModel = toGameCardViewModel(
      makeGameSummary({
        library_tags: ['steam', 'intel_xell', 'amd_fsr_frame_generation', 'dlss_super_resolution'],
      }),
    );

    expect(viewModel.libraries).toEqual([
      'steam',
      'intel_xell',
      'amd_fsr_frame_generation',
      'dlss_super_resolution',
    ]);
  });
});
