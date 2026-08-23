import { fail, requireObject, requireText } from './release-manifest-common.mjs';
import { validateExactReleaseAssetSet } from './release-manifest-github-assets.mjs';
import {
  APP_UPDATE_SESSION_ID_HEX_CHARS,
  NSIS_PLATFORM,
  PORTABLE_PLATFORM,
  PORTABLE_PROTOCOL_MAX_FRAME_BYTES,
  RELEASE_PUBLICATION_PROTOCOL,
  RELEASE_PUBLICATION_REQUIREMENTS_VERSION,
  UPDATER_MANIFEST_MAX_BYTES,
} from './release-contract.mjs';
import { validateTauriSignature } from './release-signature.mjs';
import { findReleaseHeadings } from '../ui/src/shared/model/release-note-headings.ts';
import { parseReleaseNotes } from '../ui/src/shared/model/release-notes.ts';

const SEMVER_PATTERN =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

function parseSemanticVersion(version) {
  const match = SEMVER_PATTERN.exec(version);
  if (!match) {
    return null;
  }
  const prerelease = match[4]?.split('.') ?? null;
  if (
    prerelease?.some(
      (identifier) => /^\d+$/.test(identifier) && identifier.startsWith('0') && identifier !== '0',
    )
  ) {
    return null;
  }
  return {
    core: [BigInt(match[1]), BigInt(match[2]), BigInt(match[3])],
    prerelease,
  };
}

function compareSemanticVersions(left, right) {
  for (let index = 0; index < left.core.length; index += 1) {
    if (left.core[index] !== right.core[index]) {
      return left.core[index] < right.core[index] ? -1 : 1;
    }
  }
  if (left.prerelease === null || right.prerelease === null) {
    if (left.prerelease === right.prerelease) {
      return 0;
    }
    return left.prerelease === null ? 1 : -1;
  }
  const sharedLength = Math.min(left.prerelease.length, right.prerelease.length);
  for (let index = 0; index < sharedLength; index += 1) {
    const leftIdentifier = left.prerelease[index];
    const rightIdentifier = right.prerelease[index];
    if (leftIdentifier === rightIdentifier) {
      continue;
    }
    const leftIsNumeric = /^\d+$/.test(leftIdentifier);
    const rightIsNumeric = /^\d+$/.test(rightIdentifier);
    if (leftIsNumeric && rightIsNumeric) {
      return BigInt(leftIdentifier) < BigInt(rightIdentifier) ? -1 : 1;
    }
    if (leftIsNumeric !== rightIsNumeric) {
      return leftIsNumeric ? -1 : 1;
    }
    return leftIdentifier < rightIdentifier ? -1 : 1;
  }
  return left.prerelease.length === right.prerelease.length
    ? 0
    : left.prerelease.length < right.prerelease.length
      ? -1
      : 1;
}

function selectReleaseNotes(changelogInput, releaseVersion) {
  const changelog = requireText(changelogInput, 'Release changelog');
  const scannedHeadings = findReleaseHeadings(changelog);
  const malformed = scannedHeadings.find((heading) => heading.kind === 'malformed');
  if (malformed) {
    fail(`Release changelog contains malformed level-two heading: ${malformed.source}.`);
  }
  const headings = scannedHeadings.map((heading) => ({
    index: heading.start,
    version: heading.version,
  }));
  const releaseIndex = headings.findIndex((heading) => heading.version === releaseVersion);
  if (releaseIndex < 0) {
    fail(`Release changelog does not contain a section for ${releaseVersion}.`);
  }
  const preReleaseHeadings = headings.slice(0, releaseIndex);
  if (
    preReleaseHeadings.length > 1 ||
    preReleaseHeadings.some((heading) => heading.version !== 'Unreleased')
  ) {
    const invalid = preReleaseHeadings.find((heading) => heading.version !== 'Unreleased');
    fail(
      `Release changelog heading ${invalid?.version ?? 'Unreleased'} is not an allowed release preamble.`,
    );
  }

  const releaseHeadings = headings.slice(releaseIndex);
  const seenVersions = new Set();
  let previous = null;
  for (const heading of releaseHeadings) {
    const parsed = parseSemanticVersion(heading.version);
    if (!parsed) {
      fail(`Release changelog version ${heading.version} is not valid SemVer.`);
    }
    if (seenVersions.has(heading.version)) {
      fail(`Release changelog contains duplicate version ${heading.version}.`);
    }
    if (previous && compareSemanticVersions(previous.parsed, parsed) <= 0) {
      fail(
        `Release changelog versions must be strictly newest-first: ${previous.version} precedes ${heading.version}.`,
      );
    }
    seenVersions.add(heading.version);
    previous = { parsed, version: heading.version };
  }

  const history = changelog.slice(releaseHeadings[0].index).trim();
  if (parseReleaseNotes(history).truncated) {
    fail('Release changelog exceeds the lossless UI release-notes budget.');
  }
  const currentEnd = headings[releaseIndex + 1]?.index ?? changelog.length;
  return {
    current: changelog.slice(releaseHeadings[0].index, currentEnd).trim(),
    history,
    versions: releaseHeadings.map((heading) => heading.version),
  };
}

function validatePortableCheckFrame({ currentVersions, notes, publishedAt, version }) {
  const requestId = '0'.repeat(APP_UPDATE_SESSION_ID_HEX_CHARS);
  const frameBytes = Math.max(
    ...currentVersions.map((currentVersion) =>
      Buffer.byteLength(
        `${JSON.stringify({
          type: 'update_response',
          request_id: requestId,
          response: {
            result: 'check',
            available: true,
            current_version: currentVersion,
            version,
            date: publishedAt,
            body: notes,
          },
        })}\n`,
        'utf8',
      ),
    ),
  );
  if (frameBytes > PORTABLE_PROTOCOL_MAX_FRAME_BYTES) {
    fail(
      `Updater notes require a ${frameBytes}-byte portable response, exceeding the ${PORTABLE_PROTOCOL_MAX_FRAME_BYTES}-byte protocol frame.`,
    );
  }
}

function validateUpdaterManifestSize(manifest) {
  const manifestBytes = Buffer.byteLength(`${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
  if (manifestBytes > UPDATER_MANIFEST_MAX_BYTES) {
    fail(
      `Updater manifest is ${manifestBytes} bytes, exceeding the ${UPDATER_MANIFEST_MAX_BYTES}-byte portable download limit.`,
    );
  }
}

function validateVersionContext({ repository, tag, version }) {
  if (!parseSemanticVersion(version)) {
    fail(`Release version ${version} is not valid SemVer.`);
  }
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)) {
    fail(`GitHub repository ${repository} is invalid.`);
  }
  if (tag !== `v${version}`) {
    fail(`Release tag ${tag} does not match version v${version}.`);
  }
}

function validateCommit(commit, label) {
  if (typeof commit !== 'string' || !/^[0-9a-f]{40}$/i.test(commit)) {
    fail(`${label} must be a 40-character Git commit SHA.`);
  }
  return commit.toLowerCase();
}

function validatePublishedAt(publishedAt) {
  if (
    typeof publishedAt !== 'string' ||
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(publishedAt) ||
    Number.isNaN(Date.parse(publishedAt))
  ) {
    fail('Release publication timestamp must be a stable ISO-8601 timestamp.');
  }
  return publishedAt;
}

function validateRunId(runId) {
  if (typeof runId !== 'string' || !/^\d+$/.test(runId)) {
    fail('GitHub run ID must be a non-empty decimal identifier.');
  }
  return runId;
}

function releaseUrl({ name, repository, tag }) {
  return `https://github.com/${repository}/releases/download/${tag}/${name}`;
}

/**
 * Deterministically creates the public updater manifest from local, current-run
 * artifacts. No GitHub release asset or action-generated manifest is an input.
 */
export function createLatestManifest({
  changelog,
  installerSignature,
  portableRpuSignature,
  publishedAt,
  repository,
  tag,
  version,
}) {
  validateVersionContext({ repository, tag, version });
  const releaseNotes = selectReleaseNotes(changelog, version);
  const notes = releaseNotes.history;
  const stablePublishedAt = validatePublishedAt(publishedAt);
  const installerAssetName = `RenderPilot_${version}_x64-setup.exe`;
  const portableRpuAssetName = `RenderPilot_${version}_x64-portable.rpu`;
  const manifest = {
    version,
    notes,
    pub_date: stablePublishedAt,
    platforms: {
      [NSIS_PLATFORM]: {
        signature: validateTauriSignature(installerSignature, 'NSIS installer signature'),
        url: releaseUrl({ name: installerAssetName, repository, tag }),
      },
      [PORTABLE_PLATFORM]: {
        signature: validateTauriSignature(portableRpuSignature, 'Portable RPU signature'),
        url: releaseUrl({ name: portableRpuAssetName, repository, tag }),
      },
    },
  };
  validatePortableCheckFrame({
    currentVersions: releaseNotes.versions,
    notes,
    publishedAt: stablePublishedAt,
    version,
  });
  validateUpdaterManifestSize(manifest);
  return {
    manifest,
    portableRpuAssetName,
  };
}

function provenanceMarker(provenance) {
  return `<!-- renderpilot-release-provenance:${JSON.stringify(provenance)} -->`;
}

/**
 * Returns the staging and final release metadata. The explicit marker makes a
 * retry eligible only for the same authenticated GitHub Actions run.
 */
export function createReleasePublicationSpec({
  changelog,
  commit,
  githubSha,
  publishedAt,
  repository,
  runId,
  tag,
  version,
}) {
  validateVersionContext({ repository, tag, version });
  const targetCommit = validateCommit(commit, 'Release target commit');
  const eventSha = validateCommit(githubSha, 'GitHub event SHA');
  const stableRunId = validateRunId(runId);
  const notes = selectReleaseNotes(changelog, version).current;
  validatePublishedAt(publishedAt);
  const provenance = {
    protocol: RELEASE_PUBLICATION_PROTOCOL,
    repository,
    final_tag: tag,
    github_sha: eventSha,
    run_id: stableRunId,
    requirements_version: RELEASE_PUBLICATION_REQUIREMENTS_VERSION,
  };
  const marker = provenanceMarker(provenance);
  const stagingTag = `renderpilot-staging-${tag}-${stableRunId}`;
  const final = {
    body: `${notes}\n\n${marker}`,
    draft: false,
    name: `RenderPilot v${version}`,
    prerelease: false,
    tag_name: tag,
    target_commitish: targetCommit,
  };
  return {
    final,
    final_request: { ...final, make_latest: 'true' },
    marker,
    provenance,
    requirements_version: RELEASE_PUBLICATION_REQUIREMENTS_VERSION,
    staging: {
      body: marker,
      draft: true,
      name: `RenderPilot staging ${tag} run ${stableRunId}`,
      prerelease: false,
      tag_name: stagingTag,
      target_commitish: targetCommit,
    },
  };
}

function assertReleaseMetadata(release, expected, state) {
  const actual = requireObject(release, `GitHub ${state} release`);
  if (!Number.isSafeInteger(actual.id) || actual.id <= 0) {
    fail(`GitHub ${state} release must include a positive integer release ID.`);
  }
  for (const field of ['body', 'draft', 'name', 'prerelease', 'tag_name', 'target_commitish']) {
    if (actual[field] !== expected[field]) {
      fail(`GitHub ${state} release ${field} does not match the publication specification.`);
    }
  }
  if (!Array.isArray(actual.assets)) {
    fail(`GitHub ${state} release assets must be an array.`);
  }
}

export function assertStagingRelease({ release, specification }) {
  const publication = requireObject(specification, 'Publication specification');
  assertReleaseMetadata(
    release,
    requireObject(publication.staging, 'Staging specification'),
    'staging',
  );
}

export function assertFinalPublishedRelease({ expectedAssets, release, specification }) {
  const publication = requireObject(specification, 'Publication specification');
  assertReleaseMetadata(release, requireObject(publication.final, 'Final specification'), 'final');
  validateExactReleaseAssetSet({ assets: release.assets, expectedAssets });
}

/**
 * A release fetched by ID may have been atomically renamed from the staging tag
 * to the final tag between two calls. Treat that as success only when it is the
 * exact, complete final publication; every other mutation remains fail-closed.
 */
export function classifyRefetchedPublication({ expectedAssets, release, specification }) {
  try {
    assertStagingRelease({ release, specification });
    return 'staging';
  } catch {
    try {
      assertFinalPublishedRelease({ expectedAssets, release, specification });
      return 'final';
    } catch {
      fail(
        'Refetched release is neither the exact current-run staging draft nor the exact final publication.',
      );
    }
  }
}

function selectExactlyOnePath(artifactPaths, expectedName) {
  const matches = artifactPaths.filter(
    (artifactPath) => artifactPath.split(/[\\/]/).at(-1) === expectedName,
  );
  if (matches.length !== 1) {
    fail(`Expected one current-run artifact ${expectedName}; found ${matches.length}.`);
  }
  return matches[0];
}

/** Selects only the signed Tauri v2 installer produced in this action run. */
export function selectCurrentRunInstallerArtifacts({ artifactPaths, version }) {
  if (
    !Array.isArray(artifactPaths) ||
    artifactPaths.some((artifactPath) => typeof artifactPath !== 'string')
  ) {
    fail('tauri-action artifactPaths must be an array of path strings.');
  }
  validateVersionContext({
    repository: 'owner/repository',
    tag: `v${version}`,
    version,
  });
  const installerName = `RenderPilot_${version}_x64-setup.exe`;
  return {
    installerPath: selectExactlyOnePath(artifactPaths, installerName),
    installerSignaturePath: selectExactlyOnePath(artifactPaths, `${installerName}.sig`),
  };
}
