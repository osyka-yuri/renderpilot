import { readFile, writeFile } from 'node:fs/promises';
import process from 'node:process';

import { fail, sha256 } from './release-manifest-common.mjs';
import {
  createReleaseArtifactSpecs,
  parseReleaseAssets,
  planCreateOnlyAssetUpload,
  validateUploadedReleaseAssets,
} from './release-manifest-github-assets.mjs';
import {
  assertFinalPublishedRelease,
  assertStagingRelease,
  classifyRefetchedPublication,
  createLatestManifest,
  createReleasePublicationSpec,
  selectCurrentRunInstallerArtifacts,
} from './release-manifest-policy.mjs';
import { validatePortableArtifacts } from './release-manifest-portable.mjs';

function parseOptions(args, { flags = [], repeatable = [] } = {}) {
  const flagOptions = new Set(flags);
  const repeatableOptions = new Set(repeatable);
  const options = new Map();
  for (let index = 0; index < args.length; index += 1) {
    const option = args[index];
    if (!option?.startsWith('--')) {
      fail(`Expected an option, received ${args.slice(index).join(' ')}.`);
    }
    if (flagOptions.has(option)) {
      if (options.has(option)) {
        fail(`${option} was provided more than once.`);
      }
      options.set(option, [true]);
      continue;
    }
    const value = args[index + 1];
    if (value === undefined) {
      fail(`Expected a value after ${option}.`);
    }
    if (options.has(option) && !repeatableOptions.has(option)) {
      fail(`${option} was provided more than once.`);
    }
    const values = options.get(option) ?? [];
    values.push(value);
    options.set(option, values);
    index += 1;
  }
  return {
    get(option) {
      const values = options.get(option);
      if (values?.length !== 1) {
        fail(`${option} is required exactly once.`);
      }
      return values[0];
    },
    getAll(option) {
      return options.get(option) ?? [];
    },
    has(option) {
      return options.has(option);
    },
  };
}

async function readJson(path, label) {
  let source;
  try {
    if (path === '-') {
      const chunks = [];
      for await (const chunk of process.stdin) {
        chunks.push(chunk);
      }
      source = Buffer.concat(chunks).toString('utf8');
    } else {
      source = await readFile(path, 'utf8');
    }
  } catch (error) {
    fail(`Could not read ${label}: ${error.message}`);
  }
  try {
    return JSON.parse(source);
  } catch (error) {
    fail(`${label} is invalid JSON: ${error.message}`);
  }
}

async function runTransform(args) {
  const options = parseOptions(args);
  const outputPath = options.get('--output');
  const version = options.get('--version');
  const repository = options.get('--repository');
  const tag = options.get('--tag');
  const changelogPath = options.get('--changelog');
  const publishedAt = options.get('--published-at');
  const installerPath = options.get('--installer');
  const installerSignaturePath = options.get('--installer-signature');
  const rawPath = options.get('--portable-raw');
  const rawSignaturePath = options.get('--portable-raw-signature');
  const rpuPath = options.get('--portable-rpu');
  const rpuSignaturePath = options.get('--portable-rpu-signature');
  const zipPath = options.get('--portable-zip');
  const zipEntry = options.get('--zip-entry');

  const [changelog, installer, installerSignature, artifacts] = await Promise.all([
    readFile(changelogPath, 'utf8'),
    readFile(installerPath),
    readFile(installerSignaturePath, 'utf8'),
    validatePortableArtifacts({
      rawPath,
      rawSignaturePath,
      rpuPath,
      rpuSignaturePath,
      zipEntry,
      zipPath,
      expectedVersion: version,
    }),
  ]);
  if (installer.length === 0) {
    fail('NSIS installer must not be empty.');
  }
  const transformed = createLatestManifest({
    changelog,
    installerSignature,
    portableRpuSignature: artifacts.rpuSignature,
    publishedAt,
    repository,
    tag,
    version,
  });
  await writeFile(outputPath, `${JSON.stringify(transformed.manifest, null, 2)}\n`, 'utf8');
  process.stdout.write(
    `Prepared latest.json for ${version}: ${transformed.portableRpuAssetName} ` +
      `RPU SHA-256 ${artifacts.rpuSha256}\n`,
  );
}

async function runVerifyUpload(args) {
  const options = parseOptions(args, { flags: ['--exact'], repeatable: ['--artifact'] });
  const assetsPath = options.get('--assets');
  const artifactPaths = options.getAll('--artifact');
  if (artifactPaths.length === 0) {
    fail('At least one --artifact is required.');
  }
  await validateUploadedReleaseAssets({
    assets: parseReleaseAssets(await readFile(assetsPath, 'utf8')),
    artifactPaths,
    exact: options.has('--exact'),
  });
  process.stdout.write(
    `Verified SHA-256 digests for ${artifactPaths.length} uploaded release assets.\n`,
  );
}

async function runPlanUpload(args) {
  const options = parseOptions(args);
  const releasePath = options.get('--release');
  const artifactPath = options.get('--artifact');
  const [release, artifact] = await Promise.all([
    readJson(releasePath, 'GitHub release JSON'),
    readFile(artifactPath),
  ]);
  const plan = planCreateOnlyAssetUpload({
    artifactDigest: `sha256:${sha256(artifact)}`,
    artifactName: artifactPath.split(/[\\/]/).at(-1),
    artifactSize: artifact.length,
    release,
  });
  process.stdout.write(`${JSON.stringify(plan)}\n`);
}

async function runPublicationSpec(args) {
  const options = parseOptions(args);
  const changelog = await readFile(options.get('--changelog'), 'utf8');
  const specification = createReleasePublicationSpec({
    changelog,
    commit: options.get('--commit'),
    githubSha: options.get('--github-sha'),
    publishedAt: options.get('--published-at'),
    repository: options.get('--repository'),
    runId: options.get('--run-id'),
    tag: options.get('--tag'),
    version: options.get('--version'),
  });
  process.stdout.write(`${JSON.stringify(specification)}\n`);
}

async function runAssertPublicationState(args) {
  const options = parseOptions(args, { repeatable: ['--artifact'] });
  const state = options.get('--state');
  const [release, specification] = await Promise.all([
    readJson(options.get('--release'), 'GitHub release JSON'),
    readJson(options.get('--spec'), 'Publication specification JSON'),
  ]);
  if (state === 'staging') {
    assertStagingRelease({ release, specification });
  } else if (state === 'final') {
    const artifactPaths = options.getAll('--artifact');
    if (artifactPaths.length === 0) {
      fail('Final publication verification requires at least one --artifact.');
    }
    assertFinalPublishedRelease({
      expectedAssets: await createReleaseArtifactSpecs(artifactPaths),
      release,
      specification,
    });
  } else {
    fail('--state must be staging or final.');
  }
}

async function runClassifyPublicationState(args) {
  const options = parseOptions(args, { repeatable: ['--artifact'] });
  const artifactPaths = options.getAll('--artifact');
  if (artifactPaths.length === 0) {
    fail('Refetched publication classification requires at least one --artifact.');
  }
  const [release, specification, expectedAssets] = await Promise.all([
    readJson(options.get('--release'), 'GitHub release JSON'),
    readJson(options.get('--spec'), 'Publication specification JSON'),
    createReleaseArtifactSpecs(artifactPaths),
  ]);
  const state = classifyRefetchedPublication({ expectedAssets, release, specification });
  process.stdout.write(`${JSON.stringify({ state })}\n`);
}

async function runSelectTauriArtifacts(args) {
  const options = parseOptions(args);
  const serializedPaths = options.get('--paths-json');
  let artifactPaths;
  try {
    artifactPaths = JSON.parse(serializedPaths);
  } catch (error) {
    fail(`tauri-action artifactPaths output is invalid JSON: ${error.message}`);
  }
  process.stdout.write(
    `${JSON.stringify(
      selectCurrentRunInstallerArtifacts({ artifactPaths, version: options.get('--version') }),
    )}\n`,
  );
}

async function main() {
  const [command, ...args] = process.argv.slice(2);
  if (command === 'transform') {
    await runTransform(args);
    return;
  }
  if (command === 'verify-upload') {
    await runVerifyUpload(args);
    return;
  }
  if (command === 'plan-upload') {
    await runPlanUpload(args);
    return;
  }
  if (command === 'publication-spec') {
    await runPublicationSpec(args);
    return;
  }
  if (command === 'assert-publication-state') {
    await runAssertPublicationState(args);
    return;
  }
  if (command === 'classify-publication-state') {
    await runClassifyPublicationState(args);
    return;
  }
  if (command === 'select-tauri-artifacts') {
    await runSelectTauriArtifacts(args);
    return;
  }
  fail(
    'Usage: release-manifest.mjs ' +
      '<transform|verify-upload|plan-upload|publication-spec|assert-publication-state|classify-publication-state|select-tauri-artifacts> [options].',
  );
}

if (import.meta.main) {
  main().catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
