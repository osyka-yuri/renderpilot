import { type GameCandidateGroup, type GameDetails } from '@entities/game';

import { requiresD3d12Preflight } from './d3d12-preflight';
import type { PlannedSwap } from './swap-request';

type GameCandidate = GameCandidateGroup['candidates'][number];

/**
 * The single "update everything to its latest version" action for a game.
 *
 * It reuses the same per-component swap path as the dropdowns: each entry in
 * `items` is one download-then-apply, executed by the page model's bulk runner.
 * Only genuine upgrades are included — a component already on its newest version
 * (or whose current version is unknown) contributes nothing.
 */

export type UpdateAllPlan = {
  /** Components to swap to reach their latest version (already-current excluded). */
  items: PlannedSwap[];
  /** How many components this plan updates (`items.length`). */
  updateCount: number;
};

/**
 * Builds the "update all to latest" plan from a game's components and their
 * candidate groups.
 *
 * The backend is the single owner of automatic-selection policy, including
 * composite-version partial ordering and cohesive Streamline groups. The UI only
 * resolves each returned artifact id to its already-present candidate payload.
 */
export function buildUpdateAllToLatestPlan(details: GameDetails | null): UpdateAllPlan {
  if (!details) {
    return { items: [], updateCount: 0 };
  }

  const groupsById = uniqueCandidateGroupIndex(details.candidate_groups);

  const items: PlannedSwap[] = [];

  for (const component of details.components) {
    const candidate = resolveAutomaticCandidate(groupsById.get(component.id));
    if (candidate) {
      items.push({
        kind: requiresD3d12Preflight(component.technology) ? 'd3d12' : 'direct',
        target: {
          componentId: component.id,
          artifactId: candidate.artifact_id,
          isDownloaded: candidate.is_downloaded,
        },
      });
    }
  }

  return { items, updateCount: items.length };
}

/**
 * Resolves the backend-selected automatic candidate without reimplementing its
 * eligibility, ordering, provenance, or bundle-cohesion policy.
 */
export function resolveAutomaticCandidate(
  group: GameCandidateGroup | null | undefined,
): GameCandidate | null {
  if (!group?.automatic_candidate_artifact_id) {
    return null;
  }

  const matches = group.candidates.filter(
    (candidate) => candidate.artifact_id === group.automatic_candidate_artifact_id,
  );
  return matches.length === 1 ? matches[0] : null;
}

function uniqueCandidateGroupIndex(
  groups: readonly GameCandidateGroup[],
): Map<string, GameCandidateGroup | null> {
  const index = new Map<string, GameCandidateGroup | null>();
  for (const group of groups) {
    if (index.has(group.component_id)) {
      index.set(group.component_id, null);
    } else {
      index.set(group.component_id, group);
    }
  }
  return index;
}
