import type { GameDetails, GameSummary } from './types';

/**
 * Shared test utility for creating minimal GameSummary objects.
 * Centralizes the common shape so individual tests don't duplicate the boilerplate.
 * All unstaged test files should migrate to this helper.
 */
export function createGameSummary(overrides: Partial<GameSummary> = {}): GameSummary {
  return {
    game_id: 'game:test',
    title: 'Test Game',
    launcher: 'Steam',
    // Wire values match backend serde (`Platform::Windows`, `GameRuntime::NativeWindows`).
    platform: 'Windows',
    runtime: 'NativeWindows',
    install_path: '/games/test',
    library_tags: [],
    component_count: 0,
    addon_capabilities: [],
    updates_available: false,
    update_count: 0,
    risk_level: 'safe',
    rollback_available: false,
    operation_count: 0,
    last_operation_status: null,
    cover_updated_at_ms: null,
    is_favorite: false,
    is_hidden: false,
    ...overrides,
  };
}

/**
 * Shared test utility for creating minimal GameDetails objects.
 * Mirrors createGameSummary so tests that need the richer details shape (used
 * by the game details page and its stores) do not duplicate boilerplate.
 */
export function createGameDetails(overrides: Partial<GameDetails> = {}): GameDetails {
  return {
    game: {
      identity: {
        id: 'game:test',
        title: 'Test Game',
        launcher: 'Manual',
      },
      platform: 'Windows',
      runtime: 'NativeWindows',
      install_path: '/games/test',
      executable_candidates: [],
    },
    components: [],
    candidate_groups: [],
    operations: [],
    addon_capabilities: [],
    ...overrides,
  };
}
