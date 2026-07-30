import type { GameCandidate } from '@entities/game';
import type { CatalogRelease } from '@shared/model';

import { formatReleaseVersionLabel } from './release-version-label';

export type CatalogCandidateOptionPresentation = Readonly<{
  versionLabel: string;
  componentVersions: readonly string[];
}>;

type PresentationLabels = Readonly<{
  unknown: string;
}>;

export function presentCatalogCandidateOption(
  candidate: GameCandidate,
  labels: PresentationLabels,
): CatalogCandidateOptionPresentation {
  const release = candidate.catalog_package?.release ?? null;

  return {
    versionLabel: formatReleaseVersionLabel({
      version: release?.version ?? candidate.technical_version,
      releaseLabel: release === null ? candidate.release_label : release.label,
      isDebug: candidate.is_debug,
      unknownLabel: labels.unknown,
    }),
    componentVersions: presentComponentVersions(release),
  };
}

function presentComponentVersions(release: CatalogRelease | null): string[] {
  if (!release?.components) {
    return [];
  }

  return Object.entries(release.components)
    .sort(([leftName, leftVersion], [rightName, rightVersion]) => {
      const leftIsPrimary = leftVersion === release.version;
      const rightIsPrimary = rightVersion === release.version;
      if (leftIsPrimary !== rightIsPrimary) {
        return leftIsPrimary ? -1 : 1;
      }
      return leftName.localeCompare(rightName);
    })
    .map(
      ([component, version]) =>
        `${component.charAt(0).toUpperCase()}${component.slice(1)} ${version}`,
    );
}
