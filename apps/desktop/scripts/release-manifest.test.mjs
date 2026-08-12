import assert from 'node:assert/strict';
import { execFile as execFileCallback, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { deflateRawSync } from 'node:zlib';

import { assembleRpsx1 } from './portable-rpu.mjs';
import {
  createReleaseArtifactSpecs,
  planCreateOnlyAssetUpload,
  validateExactReleaseAssetSet,
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
import { extractZipEntry } from './release-manifest-zip.mjs';

const VERSION = '1.9.0';
const REPOSITORY = 'osyka-yuri/renderpilot';
const TAG = `v${VERSION}`;
const COMMIT = 'a'.repeat(40);
const GITHUB_SHA = 'b'.repeat(40);
const RUN_ID = '123456789';
const PUBLISHED_AT = '2026-08-05T12:34:56+00:00';
const CHANGELOG = '## [1.9.0]\n\n- Portable updater support.';
const INSTALLER_ASSET = `RenderPilot_${VERSION}_x64-setup.exe`;
const INSTALLER_SIGNATURE_ASSET = `${INSTALLER_ASSET}.sig`;
const PORTABLE_ASSET = `RenderPilot_${VERSION}_x64-portable.exe`;
const PORTABLE_SIGNATURE_ASSET = `${PORTABLE_ASSET}.sig`;
const PORTABLE_RPU_ASSET = `RenderPilot_${VERSION}_x64-portable.rpu`;
const PORTABLE_RPU_SIGNATURE_ASSET = `${PORTABLE_RPU_ASSET}.sig`;
const PORTABLE_ZIP_ASSET = `RenderPilot_${VERSION}_x64-portable.zip`;
const ZIP_ENTRY = 'RenderPilot/renderpilot-desktop.exe';
const SCRIPT_PATH = fileURLToPath(new URL('./release-manifest.mjs', import.meta.url));
const WORKFLOW_PATH = fileURLToPath(
  new URL('../../../.github/workflows/release.yml', import.meta.url),
);
const PUBLISH_SCRIPT_PATH = fileURLToPath(new URL('./publish-release-assets.ps1', import.meta.url));
const GITHUB_CLIENT_PATH = fileURLToPath(new URL('./release-github-client.psm1', import.meta.url));
const TAURI_CONFIG_PATH = fileURLToPath(new URL('../src-tauri/tauri.conf.json', import.meta.url));
const execFile = promisify(execFileCallback);
const TAURI_SIGNATURE = Buffer.from(
  'untrusted comment: signature from tauri secret key\n' +
    'RURlbW8gc2lnbmF0dXJlIGJ5dGVz\n' +
    'trusted comment: timestamp:0\tfile:renderpilot-desktop.exe\n' +
    'ZGVtbyBnbG9iYWwgc2lnYXR1cmU=\n',
).toString('base64');

function crc32(buffer) {
  let result = 0xffffffff;
  for (const byte of buffer) {
    result ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      result = (result >>> 1) ^ (0xedb88320 & -(result & 1));
    }
  }
  return (result ^ 0xffffffff) >>> 0;
}

function createStoredZip(name, contents) {
  return createZip([{ contents, name }]).archive;
}

function createPortableRpu(app, version = VERSION) {
  const manifest = {
    app_length: app.length,
    app_sha256: createHash('sha256').update(app).digest('hex'),
    maximum_schema: 16,
    minimum_schema: 4,
    minimum_supervisor_protocol: 1,
    platform: 'windows-x86_64-portable',
    portable_role: 'app',
    protocol: 'renderpilot-portable-rpu-v1',
    version,
  };
  return createZip([
    { contents: JSON.stringify(manifest), name: 'rpu-manifest.json' },
    { contents: app, name: 'app/renderpilot-app.exe' },
  ]).archive;
}

function createZip(entries, { localPrefix = Buffer.alloc(0) } = {}) {
  const localParts = [localPrefix];
  const centralParts = [];
  const layout = [];
  let localOffset = localPrefix.length;

  for (const entry of entries) {
    const contents = Buffer.from(entry.contents ?? Buffer.alloc(0));
    const compression = entry.compression ?? 0;
    const compressed = compression === 8 ? deflateRawSync(contents) : contents;
    const flags = entry.flags ?? 0x0800;
    const localName = Buffer.from(entry.localNameBytes ?? entry.nameBytes ?? entry.name, 'utf8');
    const centralName = Buffer.from(
      entry.centralNameBytes ?? entry.nameBytes ?? entry.name,
      'utf8',
    );
    const localExtra = Buffer.from(entry.localExtra ?? Buffer.alloc(0));
    const centralExtra = Buffer.from(entry.centralExtra ?? Buffer.alloc(0));
    const centralComment = Buffer.from(entry.centralComment ?? Buffer.alloc(0));
    const checksum = crc32(contents);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(flags, 6);
    local.writeUInt16LE(compression, 8);
    local.writeUInt32LE(checksum, 14);
    local.writeUInt32LE(compressed.length, 18);
    local.writeUInt32LE(contents.length, 22);
    local.writeUInt16LE(localName.length, 26);
    local.writeUInt16LE(localExtra.length, 28);
    localParts.push(local, localName, localExtra, compressed);

    layout.push({
      centralOffset: undefined,
      compressedDataOffset: localOffset + 30 + localName.length + localExtra.length,
      localOffset,
    });
    localOffset += local.length + localName.length + localExtra.length + compressed.length;

    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(flags, 8);
    central.writeUInt16LE(compression, 10);
    central.writeUInt32LE(checksum, 16);
    central.writeUInt32LE(compressed.length, 20);
    central.writeUInt32LE(contents.length, 24);
    central.writeUInt16LE(centralName.length, 28);
    central.writeUInt16LE(centralExtra.length, 30);
    central.writeUInt16LE(centralComment.length, 32);
    central.writeUInt32LE(layout.at(-1).localOffset, 42);
    centralParts.push(central, centralName, centralExtra, centralComment);
  }

  const centralOffset = localOffset;
  let centralSize = 0;
  for (let index = 0; index < entries.length; index += 1) {
    layout[index].centralOffset = centralOffset + centralSize;
    centralSize +=
      46 +
      centralParts[index * 4 + 1].length +
      centralParts[index * 4 + 2].length +
      centralParts[index * 4 + 3].length;
  }
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(centralSize, 12);
  end.writeUInt32LE(centralOffset, 16);
  return {
    archive: Buffer.concat([...localParts, ...centralParts, end]),
    centralOffset,
    eocdOffset: centralOffset + centralSize,
    layout,
  };
}

function assertZipRejects(
  archive,
  pattern = /Portable ZIP|unsupported|unexpected|duplicate|invalid/,
) {
  assert.throws(() => extractZipEntry(archive, ZIP_ENTRY), pattern);
}

function publicationSpec() {
  return createReleasePublicationSpec({
    changelog: CHANGELOG,
    commit: COMMIT,
    githubSha: GITHUB_SHA,
    publishedAt: PUBLISHED_AT,
    repository: REPOSITORY,
    runId: RUN_ID,
    tag: TAG,
    version: VERSION,
  });
}

function releaseFor(specification, state, assets = []) {
  return {
    ...specification[state],
    assets,
    id: 501,
  };
}

function asset({ contents, id, name }) {
  return {
    digest: `sha256:${createHash('sha256').update(contents).digest('hex')}`,
    id,
    name,
    size: contents.length,
    state: 'uploaded',
  };
}

function runNodeWithStdin(arguments_, stdin) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, arguments_, { stdio: ['pipe', 'pipe', 'pipe'] });
    const output = [];
    const errors = [];
    child.stdout.on('data', (chunk) => output.push(chunk));
    child.stderr.on('data', (chunk) => errors.push(chunk));
    child.on('error', reject);
    child.on('close', (code) => {
      resolve({
        code,
        stderr: Buffer.concat(errors).toString('utf8'),
        stdout: Buffer.concat(output).toString('utf8'),
      });
    });
    child.stdin.end(stdin);
  });
}

function executableRunScalars(workflow) {
  const lines = workflow.split(/\r?\n/);
  const runs = [];
  for (let index = 0; index < lines.length; index += 1) {
    const match = /^(\s*)run:\s*(.*)$/.exec(lines[index]);
    if (!match) {
      continue;
    }
    const [, indentation, scalar] = match;
    if (scalar === '|' || scalar === '|-' || scalar === '>' || scalar === '>-') {
      const block = [];
      for (index += 1; index < lines.length; index += 1) {
        const line = lines[index];
        if (line.trim() && line.search(/\S/) <= indentation.length) {
          index -= 1;
          break;
        }
        block.push(line);
      }
      runs.push(block.join('\n'));
      continue;
    }
    runs.push(scalar);
  }
  return runs;
}

function assertExecutableRunTextIsExpressionFree(workflow) {
  for (const run of executableRunScalars(workflow)) {
    assert.doesNotMatch(
      run,
      /\$\{\{[\s\S]*?\}\}/,
      `workflow run text must not interpolate an expression: ${run}`,
    );
  }
}

test('creates deterministic final updater metadata from only current-run local inputs', () => {
  const inputs = {
    changelog: CHANGELOG,
    installerSignature: TAURI_SIGNATURE,
    portableRpuSignature: TAURI_SIGNATURE,
    publishedAt: PUBLISHED_AT,
    repository: REPOSITORY,
    tag: TAG,
    version: VERSION,
  };
  const first = createLatestManifest(inputs);
  const retried = createLatestManifest(inputs);

  assert.deepEqual(retried, first);
  assert.deepEqual(first.manifest, {
    version: VERSION,
    notes: CHANGELOG,
    pub_date: PUBLISHED_AT,
    platforms: {
      'windows-x86_64-nsis': {
        signature: TAURI_SIGNATURE,
        url: `https://github.com/${REPOSITORY}/releases/download/${TAG}/${INSTALLER_ASSET}`,
      },
      'windows-x86_64-portable': {
        signature: TAURI_SIGNATURE,
        url: `https://github.com/${REPOSITORY}/releases/download/${TAG}/${PORTABLE_RPU_ASSET}`,
      },
    },
  });
});

test('CLI transform writes byte-identical current-run manifests on retry', async () => {
  const directory = await mkdtemp(path.join(tmpdir(), 'renderpilot-release-cli-'));
  const app = Buffer.from('portable App image bytes');
  const rpu = createPortableRpu(app);
  const raw = assembleRpsx1({
    expectedVersion: VERSION,
    rpu,
    signature: Buffer.from(TAURI_SIGNATURE),
    supervisor: Buffer.from('stable portable supervisor bytes'),
  });
  const rawPath = path.join(directory, PORTABLE_ASSET);
  const rawSignaturePath = path.join(directory, PORTABLE_SIGNATURE_ASSET);
  const rpuPath = path.join(directory, PORTABLE_RPU_ASSET);
  const rpuSignaturePath = path.join(directory, PORTABLE_RPU_SIGNATURE_ASSET);
  const zipPath = path.join(directory, PORTABLE_ZIP_ASSET);
  const installerPath = path.join(directory, INSTALLER_ASSET);
  const installerSignaturePath = path.join(directory, INSTALLER_SIGNATURE_ASSET);
  const changelogPath = path.join(directory, 'changelog.md');
  const outputOne = path.join(directory, 'latest-one.json');
  const outputTwo = path.join(directory, 'latest-two.json');
  const argumentsFor = (output) => [
    SCRIPT_PATH,
    'transform',
    '--output',
    output,
    '--version',
    VERSION,
    '--repository',
    REPOSITORY,
    '--tag',
    TAG,
    '--changelog',
    changelogPath,
    '--published-at',
    PUBLISHED_AT,
    '--installer',
    installerPath,
    '--installer-signature',
    installerSignaturePath,
    '--portable-raw',
    rawPath,
    '--portable-raw-signature',
    rawSignaturePath,
    '--portable-rpu',
    rpuPath,
    '--portable-rpu-signature',
    rpuSignaturePath,
    '--portable-zip',
    zipPath,
    '--zip-entry',
    ZIP_ENTRY,
  ];

  try {
    await Promise.all([
      writeFile(rawPath, raw),
      writeFile(rawSignaturePath, TAURI_SIGNATURE),
      writeFile(rpuPath, rpu),
      writeFile(rpuSignaturePath, TAURI_SIGNATURE),
      writeFile(zipPath, createStoredZip(ZIP_ENTRY, raw)),
      writeFile(installerPath, Buffer.from('signed NSIS installer')),
      writeFile(installerSignaturePath, TAURI_SIGNATURE),
      writeFile(changelogPath, CHANGELOG),
    ]);
    const { stdout } = await execFile(process.execPath, argumentsFor(outputOne));
    await execFile(process.execPath, argumentsFor(outputTwo));

    assert.match(stdout, /Prepared latest\.json for 1\.9\.0/);
    assert.deepEqual(await readFile(outputOne), await readFile(outputTwo));
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test('keeps raw, RPU, signature, and portable ZIP content validation bound together', async () => {
  const directory = await mkdtemp(path.join(tmpdir(), 'renderpilot-release-portable-'));
  const app = Buffer.from('portable App image bytes');
  const rpu = createPortableRpu(app);
  const raw = assembleRpsx1({
    expectedVersion: VERSION,
    rpu,
    signature: Buffer.from(TAURI_SIGNATURE),
    supervisor: Buffer.from('stable portable supervisor bytes'),
  });
  const rawPath = path.join(directory, PORTABLE_ASSET);
  const rawSignaturePath = path.join(directory, PORTABLE_SIGNATURE_ASSET);
  const rpuPath = path.join(directory, PORTABLE_RPU_ASSET);
  const rpuSignaturePath = path.join(directory, PORTABLE_RPU_SIGNATURE_ASSET);
  const zipPath = path.join(directory, PORTABLE_ZIP_ASSET);
  try {
    await Promise.all([
      writeFile(rawPath, raw),
      writeFile(rawSignaturePath, TAURI_SIGNATURE),
      writeFile(rpuPath, rpu),
      writeFile(rpuSignaturePath, TAURI_SIGNATURE),
      writeFile(zipPath, createStoredZip(ZIP_ENTRY, raw)),
    ]);
    const result = await validatePortableArtifacts({
      expectedVersion: VERSION,
      rawPath,
      rawSignaturePath,
      rpuPath,
      rpuSignaturePath,
      zipEntry: ZIP_ENTRY,
      zipPath,
    });
    assert.equal(result.rawSha256, createHash('sha256').update(raw).digest('hex'));
    assert.equal(result.rpuSha256, createHash('sha256').update(rpu).digest('hex'));
    assert.deepEqual(extractZipEntry(await readFile(zipPath), ZIP_ENTRY), raw);

    await writeFile(zipPath, createStoredZip(ZIP_ENTRY, Buffer.from('different bytes')));
    await assert.rejects(
      validatePortableArtifacts({
        expectedVersion: VERSION,
        rawPath,
        rawSignaturePath,
        rpuPath,
        rpuSignaturePath,
        zipEntry: ZIP_ENTRY,
        zipPath,
      }),
      /does not equal the signed raw portable supervisor bytes/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test('portable ZIP parser accepts one canonical stored or deflated entry', () => {
  const executable = Buffer.from('exact portable executable bytes');
  const stored = createZip([{ contents: executable, name: ZIP_ENTRY }]);
  assert.deepEqual(extractZipEntry(stored.archive, ZIP_ENTRY), executable);

  const deflated = createZip([{ compression: 8, contents: executable, name: ZIP_ENTRY }]);
  assert.deepEqual(extractZipEntry(deflated.archive, ZIP_ENTRY), executable);
});

test('portable ZIP parser rejects directory records, extra entries, and ambiguous pathname forms', () => {
  const executable = Buffer.from('exact portable executable bytes');
  assertZipRejects(
    createZip([
      { contents: Buffer.alloc(0), name: 'RenderPilot/' },
      { contents: executable, name: ZIP_ENTRY },
    ]).archive,
    /must contain exactly one canonical portable executable entry/,
  );
  assertZipRejects(
    createZip([
      { contents: executable, name: ZIP_ENTRY },
      { contents: Buffer.from('foreign'), name: 'RenderPilot/foreign' },
    ]).archive,
    /must contain exactly one canonical portable executable entry/,
  );
  assertZipRejects(
    createZip([
      { contents: executable, name: ZIP_ENTRY },
      { contents: executable, name: ZIP_ENTRY },
    ]).archive,
    /must contain exactly one canonical portable executable entry/,
  );
  assertZipRejects(
    createZip([
      { contents: executable, name: ZIP_ENTRY },
      { contents: executable, name: 'renderpilot/renderpilot-desktop.exe' },
    ]).archive,
    /must contain exactly one canonical portable executable entry/,
  );

  for (const name of [
    'RenderPilot\\renderpilot-desktop.exe',
    '/RenderPilot/renderpilot-desktop.exe',
    'C:/RenderPilot/renderpilot-desktop.exe',
    'RenderPilot/../renderpilot-desktop.exe',
    'RenderPilot//renderpilot-desktop.exe',
    'RenderPilot/\0renderpilot-desktop.exe',
  ]) {
    assertZipRejects(createZip([{ contents: executable, name }]).archive, /canonical|NUL/);
  }
  assertZipRejects(
    createZip([
      {
        contents: executable,
        flags: 0x0800,
        nameBytes: Buffer.from([0xc3, 0x28]),
      },
    ]).archive,
    /invalid UTF-8/,
  );
  assertZipRejects(
    createZip([
      {
        contents: executable,
        flags: 0,
        nameBytes: Buffer.from([0x80]),
      },
    ]).archive,
    /ASCII/,
  );
});

test('ZIP parser rejects malformed EOCD and central-directory authority records', () => {
  const executable = Buffer.from('exact portable executable bytes');
  const mutate = (mutator) => {
    const fixture = createZip([{ contents: executable, name: ZIP_ENTRY }]);
    mutator(fixture);
    assertZipRejects(fixture.archive);
  };

  mutate(({ archive, eocdOffset }) => archive.writeUInt16LE(2, eocdOffset + 10));
  mutate(({ archive, eocdOffset }) => archive.writeUInt32LE(1, eocdOffset + 12));
  mutate(({ archive, eocdOffset }) => archive.writeUInt32LE(1, eocdOffset + 16));
  mutate(({ archive, eocdOffset }) => archive.writeUInt16LE(1, eocdOffset + 20));
  mutate(({ archive, eocdOffset }) => archive.writeUInt16LE(0xffff, eocdOffset + 10));
  mutate(({ archive, eocdOffset }) => archive.writeUInt16LE(1, eocdOffset + 4));

  assertZipRejects(
    createZip([
      {
        centralExtra: Buffer.from([0x01, 0x00, 0x00, 0x00]),
        contents: executable,
        name: ZIP_ENTRY,
      },
    ]).archive,
    /Zip64/,
  );

  const trailing = createZip([{ contents: executable, name: ZIP_ENTRY }]).archive;
  assertZipRejects(Buffer.concat([trailing, Buffer.from('trailing')]), /end-of-central-directory/);

  const malformedCentral = createZip([{ contents: executable, name: ZIP_ENTRY }]);
  malformedCentral.archive.writeUInt32LE(0, malformedCentral.layout[0].centralOffset);
  assertZipRejects(malformedCentral.archive, /central-directory record/);
});

test('ZIP parser rejects local-central disagreement and noncontiguous local data', () => {
  const executable = Buffer.from('exact portable executable bytes');
  const mutate = (
    mutator,
    pattern = /inconsistent|unsupported|crosses|gap|overlap|duplicate local/,
  ) => {
    const fixture = createZip([{ contents: executable, name: ZIP_ENTRY }]);
    mutator(fixture);
    assertZipRejects(fixture.archive, pattern);
  };

  mutate(({ archive, layout }) =>
    archive.write('RenderPilot/renderpilot-desktop.exf', layout[0].localOffset + 30),
  );
  mutate(({ archive, layout }) => archive.writeUInt16LE(0, layout[0].centralOffset + 8));
  mutate(({ archive, layout }) => archive.writeUInt16LE(8, layout[0].centralOffset + 10));
  mutate(({ archive, layout }) => archive.writeUInt32LE(0, layout[0].centralOffset + 16));
  mutate(({ archive, layout }) => archive.writeUInt32LE(1, layout[0].centralOffset + 20));
  mutate(({ archive, layout }) => archive.writeUInt32LE(1, layout[0].centralOffset + 24));
  mutate(
    ({ archive, layout }) => archive.writeUInt32LE(1, layout[0].centralOffset + 42),
    /invalid local header/,
  );
  mutate(
    ({ archive, layout }) => archive.writeUInt16LE(0x0001, layout[0].centralOffset + 8),
    /unsupported/,
  );

  const prefix = createZip([{ contents: executable, name: ZIP_ENTRY }], {
    localPrefix: Buffer.from('prefix'),
  });
  assertZipRejects(prefix.archive, /gap or overlap/);

  const duplicateLocal = createZip([
    { contents: Buffer.alloc(0), name: 'RenderPilot/' },
    { contents: executable, name: ZIP_ENTRY },
  ]);
  duplicateLocal.archive.writeUInt32LE(
    duplicateLocal.layout[0].localOffset,
    duplicateLocal.layout[1].centralOffset + 42,
  );
  assertZipRejects(
    duplicateLocal.archive,
    /must contain exactly one canonical portable executable entry/,
  );

  const crossing = createZip([{ contents: executable, name: ZIP_ENTRY }]);
  const oversized = crossing.centralOffset - crossing.layout[0].compressedDataOffset + 1;
  crossing.archive.writeUInt32LE(oversized, crossing.layout[0].localOffset + 18);
  crossing.archive.writeUInt32LE(oversized, crossing.layout[0].localOffset + 22);
  crossing.archive.writeUInt32LE(oversized, crossing.layout[0].centralOffset + 20);
  crossing.archive.writeUInt32LE(oversized, crossing.layout[0].centralOffset + 24);
  assertZipRejects(crossing.archive, /crosses into the central directory/);
});

test('selects the direct Tauri v2 NSIS updater artifacts from current-run output', async () => {
  const tauriConfig = JSON.parse(await readFile(TAURI_CONFIG_PATH, 'utf8'));
  assert.equal(tauriConfig.bundle.createUpdaterArtifacts, true);

  const selected = selectCurrentRunInstallerArtifacts({
    artifactPaths: [
      `C:/runner/${INSTALLER_ASSET}`,
      `C:/runner/${INSTALLER_SIGNATURE_ASSET}`,
      'C:/runner/ignored-local-latest.json',
    ],
    version: VERSION,
  });
  assert.equal(selected.installerPath, `C:/runner/${INSTALLER_ASSET}`);
  assert.equal(selected.installerSignaturePath, `C:/runner/${INSTALLER_SIGNATURE_ASSET}`);
  assert.throws(
    () =>
      selectCurrentRunInstallerArtifacts({
        artifactPaths: [
          `C:/runner/${INSTALLER_ASSET}`,
          `D:/runner/${INSTALLER_ASSET}`,
          `C:/runner/${INSTALLER_SIGNATURE_ASSET}`,
        ],
        version: VERSION,
      }),
    /Expected one current-run artifact RenderPilot_1\.9\.0_x64-setup\.exe/,
  );
});

test('plans only create-or-identical-skip and rejects all conflicting draft assets', () => {
  const contents = Buffer.from('release asset');
  const existing = asset({ contents, id: 100, name: 'latest.json' });
  const input = {
    artifactDigest: existing.digest,
    artifactName: existing.name,
    artifactSize: existing.size,
  };

  assert.deepEqual(planCreateOnlyAssetUpload({ ...input, release: { assets: [] } }), {
    action: 'upload',
  });
  assert.deepEqual(planCreateOnlyAssetUpload({ ...input, release: { assets: [existing] } }), {
    action: 'skip',
    assetId: 100,
  });
  assert.throws(
    () =>
      planCreateOnlyAssetUpload({
        ...input,
        artifactDigest: `sha256:${'0'.repeat(64)}`,
        release: { assets: [existing], draft: true },
      }),
    /does not match its local artifact/,
  );
  assert.throws(
    () => planCreateOnlyAssetUpload({ ...input, release: { assets: [existing, existing] } }),
    /Expected at most one release asset/,
  );

  // A 422 upload response is safe to accept only after this refetch plan changes
  // from upload to skip; a different concurrent insertion still fails above.
  const refetchedAfterConcurrentInsert = planCreateOnlyAssetUpload({
    ...input,
    release: { assets: [existing] },
  });
  assert.deepEqual(refetchedAfterConcurrentInsert, { action: 'skip', assetId: 100 });
});

test('requires an exact complete release asset set and rejects unexpected entries', () => {
  const expected = [
    asset({ contents: Buffer.from('setup'), id: 1, name: INSTALLER_ASSET }),
    asset({ contents: Buffer.from('manifest'), id: 2, name: 'latest.json' }),
  ].map(({ digest, name, size }) => ({ digest, name, size }));
  const uploaded = expected.map((specification, index) => ({
    ...specification,
    id: index + 1,
    state: 'uploaded',
  }));
  assert.doesNotThrow(() =>
    validateExactReleaseAssetSet({ assets: uploaded, expectedAssets: expected }),
  );
  assert.throws(
    () =>
      validateExactReleaseAssetSet({
        assets: [
          ...uploaded,
          asset({ contents: Buffer.from('foreign'), id: 3, name: 'foreign.exe' }),
        ],
        expectedAssets: expected,
      }),
    /expected exactly 2 release assets/,
  );
});

test('resumes only the exact current-run private staging draft', () => {
  const specification = publicationSpec();
  const staging = releaseFor(specification, 'staging');
  assert.deepEqual(specification.provenance, {
    protocol: 'renderpilot-release-publication',
    repository: REPOSITORY,
    final_tag: TAG,
    github_sha: GITHUB_SHA,
    run_id: RUN_ID,
    requirements_version: 'v1',
  });
  assert.doesNotThrow(() => assertStagingRelease({ release: staging, specification }));

  assert.throws(
    () =>
      assertStagingRelease({
        release: { ...staging, body: staging.body.replace(RUN_ID, '999') },
        specification,
      }),
    /does not match the publication specification/,
  );
  assert.throws(
    () => assertStagingRelease({ release: { ...staging, draft: false }, specification }),
    /does not match the publication specification/,
  );
});

test('CLI publication-state assertion accepts exact staging and final releases', async () => {
  const directory = await mkdtemp(path.join(tmpdir(), 'renderpilot-release-stdin-'));
  const artifactPath = path.join(directory, 'latest.json');
  const specificationPath = path.join(directory, 'publication-spec.json');
  try {
    const specification = publicationSpec();
    await Promise.all([
      writeFile(artifactPath, 'final manifest'),
      writeFile(specificationPath, JSON.stringify(specification)),
    ]);
    const stagingResult = await runNodeWithStdin(
      [
        SCRIPT_PATH,
        'assert-publication-state',
        '--state',
        'staging',
        '--release',
        '-',
        '--spec',
        specificationPath,
      ],
      JSON.stringify(releaseFor(specification, 'staging')),
    );
    assert.equal(stagingResult.code, 0, stagingResult.stderr);

    const [expectedAsset] = await createReleaseArtifactSpecs([artifactPath]);
    const finalResult = await runNodeWithStdin(
      [
        SCRIPT_PATH,
        'assert-publication-state',
        '--state',
        'final',
        '--release',
        '-',
        '--spec',
        specificationPath,
        '--artifact',
        artifactPath,
      ],
      JSON.stringify(
        releaseFor(specification, 'final', [{ ...expectedAsset, id: 804, state: 'uploaded' }]),
      ),
    );
    assert.equal(finalResult.code, 0, finalResult.stderr);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test('accepts exact final publication racing a staging-create refetch', () => {
  const specification = publicationSpec();
  const contents = Buffer.from('latest manifest');
  const expectedAssets = [
    {
      digest: `sha256:${createHash('sha256').update(contents).digest('hex')}`,
      name: 'latest.json',
      size: contents.length,
    },
  ];
  const final = releaseFor(specification, 'final', [
    { ...expectedAssets[0], id: 801, state: 'uploaded' },
  ]);

  assert.equal(
    classifyRefetchedPublication({ expectedAssets, release: final, specification }),
    'final',
  );
});

test('accepts exact final publication racing post-upload release-ID refetch', () => {
  const specification = publicationSpec();
  const contents = Buffer.from('portable updater');
  const expectedAssets = [
    {
      digest: `sha256:${createHash('sha256').update(contents).digest('hex')}`,
      name: PORTABLE_ASSET,
      size: contents.length,
    },
  ];
  const atomicallyRenamedRelease = releaseFor(specification, 'final', [
    { ...expectedAssets[0], id: 802, state: 'uploaded' },
  ]);

  assert.equal(
    classifyRefetchedPublication({
      expectedAssets,
      release: atomicallyRenamedRelease,
      specification,
    }),
    'final',
  );
});

test('CLI accepts an exact final release after publication', async () => {
  const directory = await mkdtemp(path.join(tmpdir(), 'renderpilot-release-classify-'));
  const artifactPath = path.join(directory, 'latest.json');
  const specificationPath = path.join(directory, 'publication-spec.json');
  try {
    const specification = publicationSpec();
    await Promise.all([
      writeFile(artifactPath, 'final manifest'),
      writeFile(specificationPath, JSON.stringify(specification)),
    ]);
    const [expectedAsset] = await createReleaseArtifactSpecs([artifactPath]);
    const result = await runNodeWithStdin(
      [
        SCRIPT_PATH,
        'classify-publication-state',
        '--release',
        '-',
        '--spec',
        specificationPath,
        '--artifact',
        artifactPath,
      ],
      JSON.stringify(
        releaseFor(specification, 'final', [{ ...expectedAsset, id: 803, state: 'uploaded' }]),
      ),
    );
    assert.equal(result.code, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), { state: 'final' });
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test('accepts an identical published retry and fails final-tag collisions closed', async () => {
  const directory = await mkdtemp(path.join(tmpdir(), 'renderpilot-final-state-'));
  const paths = [
    path.join(directory, INSTALLER_ASSET),
    path.join(directory, PORTABLE_ASSET),
    path.join(directory, 'latest.json'),
  ];
  try {
    await Promise.all(
      paths.map((artifactPath, index) => writeFile(artifactPath, `asset-${index}`)),
    );
    const expectedAssets = await createReleaseArtifactSpecs(paths);
    const publishedAssets = expectedAssets.map((specification, index) => ({
      ...specification,
      id: index + 1,
      state: 'uploaded',
    }));
    const specification = publicationSpec();
    const final = releaseFor(specification, 'final', publishedAssets);
    assert.doesNotThrow(() =>
      assertFinalPublishedRelease({ expectedAssets, release: final, specification }),
    );
    assert.throws(
      () =>
        assertFinalPublishedRelease({
          expectedAssets,
          release: { ...final, target_commitish: 'c'.repeat(40) },
          specification,
        }),
      /does not match the publication specification/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test('workflow and publisher enforce exact, non-destructive publication', async () => {
  const [workflow, publisher, githubClient] = await Promise.all([
    readFile(WORKFLOW_PATH, 'utf8'),
    readFile(PUBLISH_SCRIPT_PATH, 'utf8'),
    readFile(GITHUB_CLIENT_PATH, 'utf8'),
  ]);
  assert.match(workflow, /uploadUpdaterJson:\s*false/);
  assert.match(workflow, /steps\.tauri_build\.outputs\.artifactPaths/);
  assert.match(workflow, /name:\s*release-publication/);
  assert.doesNotMatch(
    workflow,
    /^\s*(?:tagName|releaseName|releaseBody|releaseDraft|prerelease|uploadUpdaterSignatures|updaterJsonPreferNsis):/m,
  );
  assert.doesNotMatch(workflow, /GITHUB_TOKEN:/);
  assert.doesNotMatch(publisher, /--clobber/);
  assert.doesNotMatch(publisher, /--method\s+DELETE/i);
  assert.doesNotMatch(publisher, /releases\/assets\//i);
  assert.doesNotMatch(publisher, /\bgh\s/);
  assert.doesNotMatch(publisher, /Split-Path\s+-LiteralPath[^\r\n]*-Leaf/);
  assert.match(publisher, /\[IO\.Path\]::GetFileName\(\$Artifact\)/);
  assert.match(publisher, /\[IO\.Path\]::GetFileName\(\$_\)/);
  assert.doesNotMatch(publisher, /Assert-RenderPilotReleaseAttestations/);
  assert.match(
    publisher,
    /Assert-TagCommit -ReleaseTag \$Tag -ExpectedCommit \$initialTagCommit\s*\n\s*\$published = Invoke-RenderPilotGitHubJson/,
  );
  const createRequest = publisher.indexOf('$created = Invoke-RenderPilotGitHubJson');
  const successfulCreate = publisher.indexOf('if ($created.Succeeded)', createRequest);
  const useCreated = publisher.indexOf('$staging = $created.Json', successfulCreate);
  const failedCreate = publisher.indexOf('else {', useCreated);
  const reconcileCreate = publisher.indexOf('$staging = Get-GitHubReleaseByTag', failedCreate);
  const validateStaging = publisher.indexOf(
    'Assert-ReleaseState -Release $staging',
    reconcileCreate,
  );
  assert.ok(
    createRequest >= 0 &&
      successfulCreate > createRequest &&
      useCreated > successfulCreate &&
      failedCreate > useCreated &&
      reconcileCreate > failedCreate &&
      validateStaging > reconcileCreate,
    'staging creation must use the POST response or reconcile a failed create before shared validation',
  );
  assert.match(publisher, /assert-publication-state/);
  assert.match(githubClient, /Invoke-WebRequest @request/);
  assert.match(githubClient, /SkipHttpErrorCheck = \$true/);
  assert.match(githubClient, /X-GitHub-Api-Version/);
  assert.match(githubClient, /2022-11-28/);
  assert.match(githubClient, /uploads\.github\.com/);
  assert.doesNotMatch(githubClient, /ValidateSet\([^)]*DELETE/);
});

test('release modules keep neutral primitives below policy and the CLI', async () => {
  const [cli, policy, portableRpu, signature] = await Promise.all([
    readFile(new URL('./release-manifest.mjs', import.meta.url), 'utf8'),
    readFile(new URL('./release-manifest-policy.mjs', import.meta.url), 'utf8'),
    readFile(new URL('./portable-rpu.mjs', import.meta.url), 'utf8'),
    readFile(new URL('./release-signature.mjs', import.meta.url), 'utf8'),
  ]);

  assert.doesNotMatch(cli, /^export\s/m);
  assert.match(policy, /from '\.\/release-contract\.mjs'/);
  assert.match(policy, /from '\.\/release-signature\.mjs'/);
  assert.match(portableRpu, /from '\.\/release-signature\.mjs'/);
  assert.match(portableRpu, /from '\.\/release-manifest-zip\.mjs'/);
  assert.doesNotMatch(portableRpu, /release-manifest-policy|release-manifest-github-assets/);
  assert.doesNotMatch(signature, /release-manifest-policy|release-manifest-github-assets/);
});

test('release workflow keeps refs out of executable interpolation and publishes only validated context', async () => {
  const workflow = await readFile(WORKFLOW_PATH, 'utf8');
  const releaseContext =
    /- name: Read release context[\s\S]*?(?=\n\s*- name: Cache Rust build)/.exec(workflow)?.[0];
  const publisher = /- name: Publish and verify release assets[\s\S]*$/.exec(workflow)?.[0];

  assert.ok(releaseContext, 'release context step is present');
  assert.ok(publisher, 'publisher step is present');
  assertExecutableRunTextIsExpressionFree(workflow);
  assert.match(releaseContext, /RENDERPILOT_RAW_REF_NAME:\s*\$\{\{\s*github\.ref_name\s*\}\}/);
  assert.match(releaseContext, /\$actualTag\s*=\s*\$env:RENDERPILOT_RAW_REF_NAME/);
  assert.match(releaseContext, /if\s*\(\$actualTag\s+-cne\s+\$expectedTag\)/);
  assert.ok(
    releaseContext.indexOf('$actualTag -cne $expectedTag') <
      releaseContext.indexOf('git rev-parse'),
    'the raw ref must pass the exact case-sensitive tag comparison before any Git command',
  );
  assert.match(releaseContext, /"tag=\$actualTag"\s*\|\s*Out-File/);
  assert.match(
    publisher,
    /RENDERPILOT_RELEASE_TAG:\s*\$\{\{\s*steps\.release_context\.outputs\.tag\s*\}\}/,
  );
  assert.match(publisher, /-Tag\s+\$env:RENDERPILOT_RELEASE_TAG/);
  assert.doesNotMatch(executableRunScalars(publisher).join('\n'), /\$\{\{/);

  const injectedRefWorkflow =
    'steps:\n  - name: crafted ref regression\n    run: git rev-parse --verify "${{ github.ref_name }}^{}"';
  assert.throws(
    () => assertExecutableRunTextIsExpressionFree(injectedRefWorkflow),
    /must not interpolate an expression/,
    'a crafted $() ref would be rejected before it can reach shell interpolation',
  );
});
