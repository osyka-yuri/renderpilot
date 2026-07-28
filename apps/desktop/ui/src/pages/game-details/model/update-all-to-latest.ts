import {
  isAutomaticCatalogCandidate,
  type GameCandidateGroup,
  type GameDetails,
} from '@entities/game';
import { comparePackageVersions } from '@shared/model';

import { NVIDIA_STREAMLINE_TECHNOLOGY } from './game-details-tabs';
import { requiresD3d12Preflight } from './d3d12-preflight';
import { buildStreamlineVersionModel } from './streamline-versions';
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

const NEWER_VERSION = 'newer_version';

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

  const items: PlannedSwap[] = [];

  // Independent components: pick the newest genuine upgrade by its full catalogue
  // package version rather than trusting the candidates' arrival order.
  for (const component of otherComponents) {
    const candidate = latestUpgrade(groupsById[component.id]);
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

  // Streamline bundle: BundleOnly plugins must share one release, so pick the
  // newest version every installed plugin can reach (`isComplete`). `options`
  // are newest-first and exclude each plugin's current version. Skipping
  // incomplete versions avoids leaving the bundle in a mixed state; the user can
  // still pick one manually from the Streamline dropdown.
  if (streamlineComponents.length > 0) {
    const model = buildStreamlineVersionModel(streamlineComponents, groupsById, 'automatic');
    const latestComplete = model.options.find((option) => option.isComplete);
    if (latestComplete) {
      items.push(
        ...latestComplete.items.map((target): PlannedSwap => ({
          kind: 'direct',
          target,
        })),
      );
    }
  }

  return { items, updateCount: items.length };
}

/**
 * The newest genuine upgrade for one component, or `null` when none exists.
 *
 * Considers only automatically eligible `newer_version` candidates and picks the
 * highest full `catalog_package.release.version`, so the result never depends on backend
 * arrival order. Automatic eligibility guarantees that this identity is present.
 */
function latestUpgrade(group: GameCandidateGroup | null | undefined): GameCandidate | null {
  const upgrades = (group?.candidates ?? []).filter(
    (candidate) =>
      candidate.comparison === NEWER_VERSION &&
      isAutomaticCatalogCandidate(candidate) &&
      candidate.d3d12_executable_action?.kind !== 'repair_required',
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
  const candidateVersion = candidate.catalog_package?.release.version;
  const bestVersion = best.catalog_package?.release.version;
  if (!candidateVersion || !bestVersion) {
    return false;
  }
  return comparePackageVersions(candidateVersion, bestVersion) > 0;
}
