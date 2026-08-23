const LEVEL_TWO_HEADING_PATTERN = /^[ \t]*##(?!#)[^\r\n]*$/gm;
const VERSIONED_RELEASE_HEADING_PATTERN =
  /^##[ \t]+\[([^\]\r\n]+)\](?:[ \t]+-[ \t]+[^\r\n]+)?[ \t]*$/;

export type VersionedReleaseHeading = {
  kind: 'versioned';
  start: number;
  version: string;
};

export type MalformedReleaseHeading = {
  kind: 'malformed';
  source: string;
  start: number;
};

export type ReleaseHeading = VersionedReleaseHeading | MalformedReleaseHeading;

/**
 * Find every level-two heading that can delimit release-note history.
 *
 * Official release headings intentionally use one canonical form. Returning
 * malformed headings explicitly lets publication reject ambiguous changelogs,
 * while consumers of legacy manifests can ignore entries they cannot identify.
 */
export function findReleaseHeadings(input: string): ReleaseHeading[] {
  return Array.from(input.matchAll(LEVEL_TWO_HEADING_PATTERN), (match) => {
    const source = match[0].trim();
    const versioned = VERSIONED_RELEASE_HEADING_PATTERN.exec(match[0]);
    if (!versioned) {
      return {
        kind: 'malformed',
        source,
        start: match.index,
      };
    }
    return {
      kind: 'versioned',
      start: match.index,
      version: versioned[1].trim(),
    };
  });
}
