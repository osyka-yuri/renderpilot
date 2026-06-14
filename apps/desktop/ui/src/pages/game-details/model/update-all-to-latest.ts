import type { GameCandidateGroup, GameDetails } from '@entities/game';

import { NVIDIA_STREAMLINE_TECHNOLOGY } from './game-details-tabs';
import {
  buildStreamlineVersionModel,
  compareVersionDesc,
  type BulkSwapItem,
} from './streamline-versions';

type GameCandidate = GameCandidateGroup['candidates'][number];

/**
 * The single "update everything to its latest version" action for a game.
 *
 * It reuses the same per-component swap path as the dropdowns: each entry in
 * `items` is one download-then-apply, executed by the page model's bulk runner.
 * Only genuine upgrades are included — a component already on its newest version
 * (or whose current version is unknown) contributes nothing.
 */

const NEWER_VERSION = 'newer_version';

export type UpdateAllPlan = {
  /** Components to swap to reach their latest version (already-current excluded). */
  items: BulkSwapItem[];
  /** How many components this plan updates (`items.length`). */
  updateCount: number;
};

/**
 * Builds the "update all to latest" plan from a game's components and their
 * candidate groups.
 *
 * Non-Streamline components are upgraded independently to their newest available
 * version. Streamline plugins are `BundleOnly` — they must all run the same
 * release — so they are upgraded together to the newest version every installed
 * plugin can reach, keeping the bundle consistent (never a mixed state).
 */
export function buildUpdateAllToLatestPlan(details: GameDetails | null): UpdateAllPlan {
  if (!details) {
    return { items: [], updateCount: 0 };
  }

  const groupsById: Record<string, GameCandidateGroup | null> = {};
  for (const group of details.candidate_groups) {
    groupsById[group.component_id] = group;
  }

  const streamlineComponents = details.components.filter(
    (component) => component.technology === NVIDIA_STREAMLINE_TECHNOLOGY,
  );
  const otherComponents = details.components.filter(
    (component) => component.technology !== NVIDIA_STREAMLINE_TECHNOLOGY,
  );

  const items: BulkSwapItem[] = [];

  // Independent components: pick the newest genuine upgrade, choosing explicitly
  // by version rather than trusting the candidates' arrival order.
  for (const component of otherComponents) {
    const candidate = latestUpgrade(groupsById[component.id]);
    if (candidate) {
      items.push({
        componentId: component.id,
        artifactId: candidate.artifact_id,
        isDownloaded: candidate.is_downloaded,
      });
    }
  }

  // Streamline bundle: BundleOnly plugins must share one release, so pick the
  // newest version every installed plugin can reach (`isComplete`). `options`
  // are newest-first and exclude each plugin's current version. Skipping
  // incomplete versions avoids leaving the bundle in a mixed state; the user can
  // still pick one manually from the Streamline dropdown.
  if (streamlineComponents.length > 0) {
    const model = buildStreamlineVersionModel(streamlineComponents, groupsById);
    const latestComplete = model.options.find((option) => option.isComplete);
    if (latestComplete) {
      items.push(...latestComplete.items);
    }
  }

  return { items, updateCount: items.length };
}

/**
 * The newest genuine upgrade for one component, or `null` when none exists.
 *
 * Considers only `newer_version` candidates and picks the highest `version`
 * explicitly (reusing the Streamline version comparator) so the result never
 * depends on the order the backend happened to return candidates in. Candidates
 * without a parseable version fall back to their relative arrival order.
 */
function latestUpgrade(group: GameCandidateGroup | null | undefined): GameCandidate | null {
  const upgrades = (group?.candidates ?? []).filter(
    (candidate) => candidate.comparison === NEWER_VERSION,
  );

  let best: GameCandidate | null = null;
  for (const candidate of upgrades) {
    if (best === null || isNewer(candidate, best)) {
      best = candidate;
    }
  }

  return best;
}

/** Whether `candidate` is a strictly newer version than the current `best`. */
function isNewer(candidate: GameCandidate, best: GameCandidate): boolean {
  if (!candidate.version || !best.version) {
    return false;
  }
  // `compareVersionDesc` orders newest-first, so a negative result means
  // `candidate` sorts ahead of (is newer than) `best`.
  return compareVersionDesc(candidate.version, best.version) < 0;
}
