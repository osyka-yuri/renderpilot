import {
  isAutomaticCatalogCandidate,
  type GameCandidate,
  type GameCandidateGroup,
  type GameGraphicsComponent,
} from '@entities/game';
import type { SwapRequest } from './swap-request';

import {
  collapseEquivalentVersions,
  compareVersionAsc,
  compareVersionDesc,
  versionsEqual,
} from './version-compare';

/** A Streamline release that can be applied across all installed components. */
export type StreamlineVersionOption = {
  /** Raw version string, e.g. `"2.4.0"`. */
  version: string;
  /** Display label, e.g. `"v2.4.0"`. */
  label: string;
  /** Every installed component is already known to be on this version. */
  isCurrent: boolean;
  /** Components that will be swapped to reach this version. */
  items: SwapRequest[];
  /** How many components this swap updates (`items.length`). */
  updateCount: number;
  /** Components that cannot reach this version with a candidate. */
  missingCount: number;
  /** Every installed component can reach this version. */
  isComplete: boolean;
  /** Every required replacement is already available locally. */
  allDownloaded: boolean;
};

export type StreamlineVersionModel = {
  /** All selectable versions, newest first. */
  options: StreamlineVersionOption[];
  /** Common installed version when every component reports the same release. */
  currentVersion: string | null;
  /** Backend reports, or component reports together, prove version divergence. */
  isMixed: boolean;
  /** Known min/max label for a mixed set. */
  versionRange: { min: string; max: string } | null;
  /** Number of installed Streamline components (normally one bundle). */
  totalCount: number;
};

export type CandidateSelectionMode = 'manual' | 'automatic';

/**
 * Builds the bulk-version model from backend-owned `version_report` values.
 * The UI never re-derives Streamline state from raw PE file metadata: the
 * backend is the only authority for known, mixed, and unknown classification.
 */
export function buildStreamlineVersionModel(
  components: GameGraphicsComponent[],
  groupsById: Record<string, GameCandidateGroup | null>,
  selectionMode: CandidateSelectionMode = 'manual',
): StreamlineVersionModel {
  const reports = components.map((component) => groupsById[component.id]?.version_report);
  const installed = summarizeInstalledVersions(reports);
  const versions = new Set<string>(installed.knownVersions);

  for (const component of components) {
    for (const candidate of groupsById[component.id]?.candidates ?? []) {
      const presentationVersion =
        candidate.catalog_package?.release.version ?? candidate.technical_version;
      if (presentationVersion && candidateMatchesMode(candidate, selectionMode)) {
        versions.add(presentationVersion);
      }
    }
  }

  const options = collapseEquivalentVersions([...versions])
    .sort(compareVersionDesc)
    .map((version) => buildOption(version, components, groupsById, selectionMode));

  return {
    options,
    currentVersion: installed.currentVersion,
    isMixed: installed.isMixed,
    versionRange: installed.versionRange,
    totalCount: components.length,
  };
}

type InstalledVersionSummary = {
  knownVersions: string[];
  currentVersion: string | null;
  isMixed: boolean;
  versionRange: { min: string; max: string } | null;
};

function summarizeInstalledVersions(
  reports: (GameCandidateGroup['version_report'] | undefined)[],
): InstalledVersionSummary {
  const knownVersions: string[] = [];
  const rangeVersions: string[] = [];
  let hasMixedReport = false;
  let hasUnknownReport = false;

  for (const report of reports) {
    if (!report || report.kind === 'unknown') {
      hasUnknownReport = true;
      continue;
    }
    if (report.kind === 'known') {
      const presentationVersion = report.catalog_release?.version ?? report.technical_version;
      if (presentationVersion === null) {
        hasUnknownReport = true;
        continue;
      }
      knownVersions.push(presentationVersion);
      rangeVersions.push(presentationVersion);
      continue;
    }

    hasMixedReport = true;
    rangeVersions.push(report.min_technical_version, report.max_technical_version);
  }

  const distinctKnown = collapseEquivalentVersions(knownVersions);
  const isMixed = hasMixedReport || distinctKnown.length > 1;
  const versionRange = isMixed ? mixedVersionRange(rangeVersions) : null;
  const currentVersion =
    !isMixed && !hasUnknownReport && distinctKnown.length === 1 ? distinctKnown[0] : null;

  return { knownVersions, currentVersion, isMixed, versionRange };
}

function mixedVersionRange(versions: string[]): { min: string; max: string } | null {
  const distinct = collapseEquivalentVersions(versions);
  if (distinct.length < 2) {
    return null;
  }
  const sorted = [...distinct].sort(compareVersionAsc);
  return { min: sorted[0], max: sorted[sorted.length - 1] };
}

function buildOption(
  version: string,
  components: GameGraphicsComponent[],
  groupsById: Record<string, GameCandidateGroup | null>,
  selectionMode: CandidateSelectionMode,
): StreamlineVersionOption {
  const items: SwapRequest[] = [];
  let missingCount = 0;
  let allDownloaded = true;
  let fullyOnVersionCount = 0;

  for (const component of components) {
    const group = groupsById[component.id];
    if (componentFullyOnVersion(group, version)) {
      fullyOnVersionCount += 1;
      continue;
    }

    const candidate = (group?.candidates ?? []).find(
      (entry) =>
        (entry.catalog_package?.release.version ?? entry.technical_version) !== null &&
        versionsEqual(
          entry.catalog_package?.release.version ?? entry.technical_version ?? '',
          version,
        ) &&
        candidateMatchesMode(entry, selectionMode),
    );
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
    isCurrent: fullyOnVersionCount === components.length,
    items,
    updateCount: items.length,
    missingCount,
    isComplete: missingCount === 0,
    allDownloaded: items.length === 0 || allDownloaded,
  };
}

function candidateMatchesMode(
  candidate: GameCandidate,
  selectionMode: CandidateSelectionMode,
): boolean {
  return (
    selectionMode === 'manual' ||
    (candidate.comparison === 'newer_version' && isAutomaticCatalogCandidate(candidate))
  );
}

function componentFullyOnVersion(
  group: GameCandidateGroup | null | undefined,
  version: string,
): boolean {
  const report = group?.version_report;
  return (
    report?.kind === 'known' &&
    (report.catalog_release?.version ?? report.technical_version) !== null &&
    versionsEqual(report.catalog_release?.version ?? report.technical_version ?? '', version)
  );
}
