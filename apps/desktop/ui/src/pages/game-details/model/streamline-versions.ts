import type {
  CoordinatedCandidateOption,
  GameCandidateGroup,
  GameLibraryComponent,
} from '@entities/game';

import { formatReleaseVersionLabel } from './release-version-label';
import {
  compareVersionAsc,
  compareVersionDesc,
  collapseEquivalentVersions,
} from './version-compare';
import type { SwapRequest } from './swap-request';

/** A backend-coordinated Streamline release that can be safely applied. */
export type StreamlineVersionOption = {
  /** Opaque backend selection identity, never a display version. */
  optionId: string;
  version: string;
  label: string;
  items: SwapRequest[];
  allDownloaded: boolean;
};

export type StreamlineVersionModel = {
  options: StreamlineVersionOption[];
  currentVersion: string | null;
  isMixed: boolean;
  versionRange: { min: string; max: string } | null;
  totalCount: number;
};

/**
 * Builds the Streamline control from backend-owned state and coordinated
 * options. The UI may resolve a candidate only by its exact artifact id; it
 * never infers a cohort from a display version or candidate order.
 */
export function buildStreamlineVersionModel(
  components: GameLibraryComponent[],
  groupsById: Record<string, GameCandidateGroup | null>,
  coordinatedOptions: readonly CoordinatedCandidateOption[] = [],
): StreamlineVersionModel {
  const reports = components.map((component) => groupsById[component.id]?.version_report);
  const installed = summarizeInstalledVersions(reports);
  const componentIds = new Set(components.map((component) => component.id));
  const options = coordinatedOptions
    .map((option) => resolveOption(option, groupsById, componentIds))
    .filter((option): option is StreamlineVersionOption => option !== null)
    .sort(
      (left, right) =>
        compareVersionDesc(left.version, right.version) ||
        left.optionId.localeCompare(right.optionId),
    );

  return {
    options,
    currentVersion: installed.currentVersion,
    isMixed: installed.isMixed,
    versionRange: installed.versionRange,
    totalCount: components.length,
  };
}

function resolveOption(
  option: CoordinatedCandidateOption,
  groupsById: Record<string, GameCandidateGroup | null>,
  componentIds: ReadonlySet<string>,
): StreamlineVersionOption | null {
  if (option.items.length !== componentIds.size) {
    return null;
  }

  const items: SwapRequest[] = [];
  let allDownloaded = true;
  let previousComponentId = '';

  for (const item of option.items) {
    if (item.component_id <= previousComponentId || !componentIds.has(item.component_id)) {
      return null;
    }
    previousComponentId = item.component_id;
    const candidate = groupsById[item.component_id]?.candidates.find(
      (entry) => entry.artifact_id === item.artifact_id,
    );
    if (!candidate) {
      return null;
    }
    items.push({
      componentId: item.component_id,
      artifactId: item.artifact_id,
      isDownloaded: candidate.is_downloaded,
    });
    allDownloaded &&= candidate.is_downloaded;
  }

  return {
    optionId: option.option_id,
    version: option.release.version,
    label: formatReleaseVersionLabel({
      version: option.release.version,
      releaseLabel: option.release.label,
      isDebug: option.release.channel === 'debug',
      unknownLabel: option.release.version,
    }),
    items,
    allDownloaded,
  };
}

type InstalledVersionSummary = {
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
      const version = report.catalog_release?.version ?? report.technical_version;
      if (version === null) {
        hasUnknownReport = true;
        continue;
      }
      knownVersions.push(version);
      rangeVersions.push(version);
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

  return { currentVersion, isMixed, versionRange };
}

function mixedVersionRange(versions: string[]): { min: string; max: string } | null {
  const distinct = collapseEquivalentVersions(versions);
  if (distinct.length < 2) {
    return null;
  }
  const sorted = [...distinct].sort(compareVersionAsc);
  return { min: sorted[0], max: sorted[sorted.length - 1] };
}
