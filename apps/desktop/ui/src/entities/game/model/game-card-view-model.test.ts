import { describe, expect, it } from 'vitest';
import type { GameSummary } from './types';
import { toGameCardViewModel } from './game-card-view-model';

function createGameSummary(overrides: Partial<GameSummary> = {}): GameSummary {
  return {
    game_id: 'steam:1',
    title: 'Test Game',
    launcher: 'Steam',
    platform: 'windows',
    runtime: 'dx12',
    install_path: 'C:/Games/Test Game',
    library_tags: [],
    component_count: 0,
    updates_available: false,
    update_count: 0,
    risk_level: 'safe',
    backup_available: false,
    operation_count: 0,
    last_operation_status: null,
    cover_updated_at_ms: null,
    ...overrides,
  };
}

describe('game-card-view-model', () => {
  it('keeps raw library tags for UI-level formatting', () => {
    const viewModel = toGameCardViewModel(
      createGameSummary({
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