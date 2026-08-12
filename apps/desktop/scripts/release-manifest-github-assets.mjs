import { readFile } from 'node:fs/promises';
import path from 'node:path';

import { fail, requireObject, sha256 } from './release-manifest-common.mjs';

export function parseReleaseAssets(source) {
  let assets;
  try {
    assets = JSON.parse(source);
  } catch (error) {
    fail(`Release assets JSON is invalid: ${error.message}`);
  }
  if (!Array.isArray(assets)) {
    fail('Release assets JSON must be an array.');
  }
  return assets;
}

export function createReleaseArtifactSpec({ digest, name, size }) {
  if (typeof name !== 'string' || name.length === 0 || /[\\/]/.test(name)) {
    fail('Release artifact name must be a non-empty filename.');
  }
  if (!Number.isSafeInteger(size) || size <= 0) {
    fail(`Release artifact ${name} must have a non-empty size.`);
  }
  if (typeof digest !== 'string' || !/^sha256:[0-9a-f]{64}$/.test(digest)) {
    fail(`Release artifact ${name} must include a SHA-256 digest.`);
  }
  return { digest, name, size };
}

function releaseAssetByName(assets, name) {
  const matches = assets.filter((asset) => asset?.name === name);
  if (matches.length !== 1) {
    fail(`Expected exactly one uploaded release asset named ${name}; found ${matches.length}.`);
  }
  const asset = requireObject(matches[0], `Release asset ${name}`);
  if (asset.state !== 'uploaded' || !Number.isSafeInteger(asset.size) || asset.size <= 0) {
    fail(`Release asset ${name} is not a non-empty uploaded asset.`);
  }
  if (typeof asset.digest !== 'string' || !/^sha256:[0-9a-f]{64}$/.test(asset.digest)) {
    fail(`Release asset ${name} must include a SHA-256 digest.`);
  }
  return asset;
}

function validateAssetMatchesSpec(asset, specification) {
  if (asset.size !== specification.size) {
    fail(`Release asset ${specification.name} size does not match its local artifact.`);
  }
  if (asset.digest !== specification.digest) {
    fail(`Release asset ${specification.name} SHA-256 does not match its local artifact.`);
  }
}

export function validateExactReleaseAssetSet({ assets, expectedAssets }) {
  if (!Array.isArray(assets)) {
    fail('GitHub release assets must be an array.');
  }
  if (!Array.isArray(expectedAssets) || expectedAssets.length === 0) {
    fail('Expected release assets must be a non-empty array.');
  }

  const expectedByName = new Map();
  for (const expected of expectedAssets) {
    const specification = createReleaseArtifactSpec(expected);
    if (expectedByName.has(specification.name)) {
      fail(`Expected release artifact ${specification.name} was provided more than once.`);
    }
    expectedByName.set(specification.name, specification);
  }
  if (assets.length !== expectedByName.size) {
    const expectedCount = expectedByName.size;
    fail(
      `Release asset set has ${assets.length} assets; expected exactly ${expectedCount} release assets.`,
    );
  }

  for (const [name, expected] of expectedByName) {
    validateAssetMatchesSpec(releaseAssetByName(assets, name), expected);
  }

  for (const asset of assets) {
    if (!expectedByName.has(asset?.name)) {
      fail(`Release contains unexpected asset ${String(asset?.name)}.`);
    }
  }
}

/**
 * Plans exactly one create-only asset upload.  A matching byte-for-byte asset
 * is an idempotent retry; every collision with different bytes fails closed.
 */
export function planCreateOnlyAssetUpload({ artifactDigest, artifactName, artifactSize, release }) {
  const releaseObject = requireObject(release, 'GitHub release');
  if (!Array.isArray(releaseObject.assets)) {
    fail('GitHub release assets must be an array.');
  }
  const specification = createReleaseArtifactSpec({
    digest: artifactDigest,
    name: artifactName,
    size: artifactSize,
  });
  const matches = releaseObject.assets.filter((asset) => asset?.name === specification.name);
  if (matches.length === 0) {
    return { action: 'upload' };
  }
  if (matches.length !== 1) {
    fail(
      `Expected at most one release asset named ${specification.name}; found ${matches.length}.`,
    );
  }

  const asset = releaseAssetByName(releaseObject.assets, specification.name);
  validateAssetMatchesSpec(asset, specification);
  if (!Number.isSafeInteger(asset.id) || asset.id <= 0) {
    fail(`Release asset ${specification.name} must include a positive integer GitHub asset ID.`);
  }
  return { action: 'skip', assetId: asset.id };
}

export async function createReleaseArtifactSpecs(artifactPaths) {
  if (!Array.isArray(artifactPaths) || artifactPaths.length === 0) {
    fail('At least one local release artifact is required.');
  }
  return Promise.all(
    artifactPaths.map(async (artifactPath) => {
      const contents = await readFile(artifactPath);
      return createReleaseArtifactSpec({
        digest: `sha256:${sha256(contents)}`,
        name: path.basename(artifactPath),
        size: contents.length,
      });
    }),
  );
}

export async function validateUploadedReleaseAssets({ assets, artifactPaths, exact = false }) {
  const expectedAssets = await createReleaseArtifactSpecs(artifactPaths);
  if (exact) {
    validateExactReleaseAssetSet({ assets, expectedAssets });
    return;
  }
  for (const expected of expectedAssets) {
    validateAssetMatchesSpec(releaseAssetByName(assets, expected.name), expected);
  }
}
