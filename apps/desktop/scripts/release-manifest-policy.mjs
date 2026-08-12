import { fail, requireObject, requireText } from './release-manifest-common.mjs';
import { validateExactReleaseAssetSet } from './release-manifest-github-assets.mjs';
import {
  NSIS_PLATFORM,
  PORTABLE_PLATFORM,
  RELEASE_PUBLICATION_PROTOCOL,
  RELEASE_PUBLICATION_REQUIREMENTS_VERSION,
} from './release-contract.mjs';
import { validateTauriSignature } from './release-signature.mjs';

function validateVersionContext({ repository, tag, version }) {
  if (
    !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.test(
      version,
    )
  ) {
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
  const notes = requireText(changelog, 'Release changelog');
  const installerAssetName = `RenderPilot_${version}_x64-setup.exe`;
  const portableRpuAssetName = `RenderPilot_${version}_x64-portable.rpu`;
  return {
    manifest: {
      version,
      notes,
      pub_date: validatePublishedAt(publishedAt),
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
    },
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
  const notes = requireText(changelog, 'Release changelog');
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
