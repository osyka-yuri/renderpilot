import type { GameCandidateGroup, GameDetails, GameSummary } from '@entities/game';
import { isKnownLibrary } from '@shared/graphics';

import { unique } from './desktop-utils';

export type GameSummaryBuildOverrides = Pick<
  GameSummary,
  'risk_level' | 'rollback_available' | 'last_operation_status'
>;

/**
 * Builds a catalog card summary from full details, matching runtime semantics:
 * only known libraries contribute to tags / update counts.
 */
export function createGameSummaryFromDetails(
  details: GameDetails,
  overrides: GameSummaryBuildOverrides,
): GameSummary {
  const visibleComponents = details.components.filter((component) =>
    isKnownLibrary(component.technology),
  );
  const visibleComponentIds = new Set(visibleComponents.map((component) => component.id));
  const visibleCandidateGroups = details.candidate_groups.filter((group) =>
    visibleComponentIds.has(group.component_id),
  );

  return {
    game_id: details.game.identity.id,
    title: details.game.identity.title,
    launcher: details.game.identity.launcher,
    platform: details.game.platform,
    runtime: details.game.runtime,
    install_path: details.game.install_path,
    external_id: details.game.identity.external_id,
    library_tags: unique(visibleComponents.map((component) => component.technology.trim())),
    component_count: visibleComponents.length,
    addon_capabilities: [...details.addon_capabilities],
    updates_available: countAvailableUpdates(visibleCandidateGroups) > 0,
    update_count: countAvailableUpdates(visibleCandidateGroups),
    risk_level: overrides.risk_level,
    rollback_available: overrides.rollback_available,
    operation_count: details.operations.length,
    last_operation_status: overrides.last_operation_status,
    cover_updated_at_ms: null,
    is_favorite: false,
    is_hidden: false,
  };
}

export function getLatestOperationStatus(
  details: GameDetails,
): GameSummary['last_operation_status'] {
  if (details.operations.length === 0) {
    return null;
  }

  return details.operations[0].status;
}

function countAvailableUpdates(candidateGroups: readonly GameCandidateGroup[]): number {
  return candidateGroups.filter((group) =>
    group.candidates.some((candidate) => candidate.comparison === 'newer_version'),
  ).length;
}
