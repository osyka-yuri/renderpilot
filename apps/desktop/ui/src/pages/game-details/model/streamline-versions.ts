import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';

/**
 * Streamline plugins (`sl.*.dll`) are `BundleOnly`: they must all run on the same
 * release. The backend now groups them into a single Streamline component and
 * swaps the whole bundle as a unit, but this module still presents the available
 * releases as one list of versions that can be applied across every plugin at
 * once — the safe bundle swap.
 *
 * A candidate never repeats a plugin's *current* version (the backend filters by
 * content hash), so a plugin already on the target version simply contributes
 * nothing to apply.
 */

/** One plugin's swap target within a bulk Streamline version change. */
export type BulkSwapItem = {
  componentId: string;
  artifactId: string;
  isDownloaded: boolean;
};

/** A Streamline release that can be applied across all installed plugins. */
export type StreamlineVersionOption = {
  /** Raw version string, e.g. `"2.4.0"`. */
  version: string;
  /** Display label, e.g. `"v2.4.0"`. */
  label: string;
  /** Plugins that will be swapped to reach this version (excludes already-current). */
  items: BulkSwapItem[];
  /** How many plugins this swap updates (`items.length`). */
  updateCount: number;
  /** Installed plugins that can't reach this version (no candidate, not already on it). */
  missingCount: number;
  /** Every installed plugin ends on this version after applying. */
  isComplete: boolean;
  /** Every plugin to update already has its artifact downloaded locally. */
  allDownloaded: boolean;
};

export type StreamlineVersionModel = {
  /** Applicable versions, newest first; only versions that change something. */
  options: StreamlineVersionOption[];
  /** Common current version across all plugins, or `null` when mixed/unknown. */
  currentVersion: string | null;
  /** Plugins are currently on differing versions. */
  isMixed: boolean;
  /** Number of installed Streamline plugins. */
  totalCount: number;
};

/**
 * Builds the bulk-version model for a set of Streamline plugin components and
 * their candidate groups.
 */
export function buildStreamlineVersionModel(
  components: GameGraphicsComponent[],
  groupsById: Record<string, GameCandidateGroup | null>,
): StreamlineVersionModel {
  const totalCount = components.length;

  const currentVersions = components.map(
    (component) => groupsById[component.id]?.current_version ?? null,
  );
  const distinctCurrent = [
    ...new Set(currentVersions.filter((version): version is string => version != null)),
  ];
  const allKnown = currentVersions.every((version) => version != null);
  const currentVersion = allKnown && distinctCurrent.length === 1 ? distinctCurrent[0] : null;
  const isMixed = distinctCurrent.length > 1;

  const versions = new Set<string>();
  for (const component of components) {
    for (const candidate of groupsById[component.id]?.candidates ?? []) {
      if (candidate.version) {
        versions.add(candidate.version);
      }
    }
  }

  const options = [...versions]
    .sort(compareVersionDesc)
    .map((version) => buildOption(version, components, groupsById))
    .filter((option) => option.updateCount > 0);

  return { options, currentVersion, isMixed, totalCount };
}

function buildOption(
  version: string,
  components: GameGraphicsComponent[],
  groupsById: Record<string, GameCandidateGroup | null>,
): StreamlineVersionOption {
  const items: BulkSwapItem[] = [];
  let missingCount = 0;
  let allDownloaded = true;

  for (const component of components) {
    const group = groupsById[component.id];

    // Already on the target version (no candidate exists for it) — nothing to do.
    if ((group?.current_version ?? null) === version) {
      continue;
    }

    const candidate = (group?.candidates ?? []).find((entry) => entry.version === version);
    if (!candidate) {
      missingCount += 1;
      continue;
    }

    items.push({
      componentId: component.id,
      artifactId: candidate.artifact_id,
      isDownloaded: candidate.is_downloaded,
    });
    if (!candidate.is_downloaded) {
      allDownloaded = false;
    }
  }

  return {
    version,
    label: `v${version}`,
    items,
    updateCount: items.length,
    missingCount,
    isComplete: missingCount === 0,
    allDownloaded,
  };
}

/** Orders dotted version strings newest-first, with a string fallback per segment. */
export function compareVersionDesc(left: string, right: string): number {
  const leftParts = left.split('.');
  const rightParts = right.split('.');
  const length = Math.max(leftParts.length, rightParts.length);

  for (let index = 0; index < length; index += 1) {
    const rawLeft = leftParts[index] ?? '0';
    const rawRight = rightParts[index] ?? '0';
    const numLeft = Number(rawLeft);
    const numRight = Number(rawRight);

    if (Number.isFinite(numLeft) && Number.isFinite(numRight)) {
      if (numLeft !== numRight) {
        return numRight - numLeft;
      }
    } else {
      const compared = rawRight.localeCompare(rawLeft);
      if (compared !== 0) {
        return compared;
      }
    }
  }

  return 0;
}
