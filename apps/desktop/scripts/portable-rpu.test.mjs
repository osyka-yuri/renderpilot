import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  RPSX1_FOOTER_BYTES,
  assembleRpsx1,
  parseRpsx1,
  validatePortableRpuArtifacts,
} from './portable-rpu.mjs';
import {
  CURRENT_PORTABLE_SCHEMA,
  MINIMUM_PORTABLE_SCHEMA,
  PORTABLE_APP_SESSION_PROTOCOL,
  PORTABLE_SUPERVISOR_CAPABILITY,
} from './portable-runtime-release-contract.mjs';

const VERSION = '1.9.0';
const ZIP_ENTRY = 'RenderPilot/renderpilot-desktop.exe';
const SIGNATURE = Buffer.from(
  'untrusted comment: portable test signature\n' +
    'RURlbW8gc2lnbmF0dXJlIGJ5dGVz\n' +
    'trusted comment: timestamp:0\tfile:renderpilot-app.exe\n' +
    'ZGVtbyBnbG9iYWwgc2lnYXR1cmU=\n',
).toString('base64');

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

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

function storedZip(entries) {
  const locals = [];
  const central = [];
  let localOffset = 0;
  for (const { contents, name } of entries) {
    const body = Buffer.from(contents);
    const nameBytes = Buffer.from(name, 'utf8');
    const checksum = crc32(body);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0x0800, 6);
    local.writeUInt32LE(checksum, 14);
    local.writeUInt32LE(body.length, 18);
    local.writeUInt32LE(body.length, 22);
    local.writeUInt16LE(nameBytes.length, 26);
    locals.push(local, nameBytes, body);

    const record = Buffer.alloc(46);
    record.writeUInt32LE(0x02014b50, 0);
    record.writeUInt16LE(20, 4);
    record.writeUInt16LE(20, 6);
    record.writeUInt16LE(0x0800, 8);
    record.writeUInt32LE(checksum, 16);
    record.writeUInt32LE(body.length, 20);
    record.writeUInt32LE(body.length, 24);
    record.writeUInt16LE(nameBytes.length, 28);
    record.writeUInt32LE(localOffset, 42);
    central.push(record, nameBytes);
    localOffset += local.length + nameBytes.length + body.length;
  }
  const centralBytes = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(centralBytes.length, 12);
  end.writeUInt32LE(localOffset, 16);
  return Buffer.concat([...locals, centralBytes, end]);
}

function manifest(app, overrides = {}) {
  return {
    protocol: 'renderpilot-portable-rpu-v1',
    platform: 'windows-x86_64-portable',
    version: VERSION,
    app_sha256: sha256(app),
    app_length: app.length,
    minimum_supervisor_protocol: PORTABLE_SUPERVISOR_CAPABILITY,
    app_session_protocol: PORTABLE_APP_SESSION_PROTOCOL,
    minimum_schema: MINIMUM_PORTABLE_SCHEMA,
    maximum_schema: CURRENT_PORTABLE_SCHEMA,
    portable_role: 'app',
    ...overrides,
  };
}

function rpuWith(overrides = {}, extraEntries = []) {
  const app = Buffer.from('MZ exact portable App fixture');
  return storedZip([
    { name: 'rpu-manifest.json', contents: JSON.stringify(manifest(app, overrides)) },
    { name: 'app/renderpilot-app.exe', contents: app },
    ...extraEntries,
  ]);
}

function rawOverlay(rpu, signature = Buffer.from(SIGNATURE)) {
  const supervisor = Buffer.from('MZ exact stable supervisor');
  const rpuOffset = supervisor.length;
  const signatureOffset = rpuOffset + rpu.length;
  const footer = Buffer.alloc(RPSX1_FOOTER_BYTES);
  footer.write('RPSX1', 0, 'ascii');
  footer.writeUInt8(1, 5);
  footer.writeBigUInt64LE(BigInt(rpuOffset), 6);
  footer.writeBigUInt64LE(BigInt(rpu.length), 14);
  footer.writeBigUInt64LE(BigInt(signatureOffset), 22);
  footer.writeBigUInt64LE(BigInt(signature.length), 30);
  Buffer.from(sha256(rpu), 'hex').copy(footer, 38);
  Buffer.from(sha256(signature), 'hex').copy(footer, 70);
  return Buffer.concat([supervisor, rpu, signature, footer]);
}

async function withArtifact({ rpu, publicRpu = rpu, raw = rawOverlay(rpu), zipRaw = raw }, action) {
  const directory = await mkdtemp(path.join(tmpdir(), 'renderpilot-rpu-test-'));
  const paths = {
    rawPath: path.join(directory, 'RenderPilot_1.9.0_x64-portable.exe'),
    rpuPath: path.join(directory, 'RenderPilot_1.9.0_x64-portable.rpu'),
    signaturePath: path.join(directory, 'RenderPilot_1.9.0_x64-portable.rpu.sig'),
    zipPath: path.join(directory, 'RenderPilot_1.9.0_x64-portable.zip'),
    zipEntry: ZIP_ENTRY,
    expectedVersion: VERSION,
  };
  try {
    await Promise.all([
      writeFile(paths.rawPath, raw),
      writeFile(paths.rpuPath, publicRpu),
      writeFile(paths.signaturePath, SIGNATURE),
      writeFile(paths.zipPath, storedZip([{ name: ZIP_ENTRY, contents: zipRaw }])),
    ]);
    await action(paths);
  } finally {
    await rm(directory, { force: true, recursive: true });
  }
}

test('RPSX1 release fixture binds exact raw, public RPU, signature, ZIP, and canonical version', async () => {
  const rpu = rpuWith();
  const raw = assembleRpsx1({
    expectedVersion: VERSION,
    rpu,
    signature: Buffer.from(SIGNATURE),
    supervisor: Buffer.from('MZ exact stable supervisor'),
  });
  const parsed = parseRpsx1(raw);
  assert.deepEqual(parsed.rpu, rpu);
  assert.deepEqual(parsed.signature, Buffer.from(SIGNATURE));
  await withArtifact({ raw, rpu }, async (paths) => {
    const result = await validatePortableRpuArtifacts(paths);
    assert.equal(result.version, VERSION);
    assert.equal(result.rawSha256, sha256(raw));
    assert.equal(result.rpuSha256, sha256(rpu));
  });
});

test('RPSX1 rejects malformed bounds and modified embedded digests', () => {
  const rpu = rpuWith();
  const bounded = rawOverlay(rpu);
  const footer = bounded.length - RPSX1_FOOTER_BYTES;
  bounded.writeBigUInt64LE(BigInt(Number.MAX_SAFE_INTEGER), footer + 6);
  assert.throws(() => parseRpsx1(bounded), /outside the RPSX1 payload range/);

  const digest = rawOverlay(rpu);
  digest[footer + 38] ^= 0xff;
  assert.throws(() => parseRpsx1(digest), /digest does not match/);
});

test('RPU rejects empty malformed noncanonical and context-mismatched versions', async () => {
  for (const version of ['', '1.2', 'v1.2.3', '01.2.3', '1.02.3', '1.2.03']) {
    const rpu = rpuWith({ version });
    await withArtifact({ rpu }, async (paths) => {
      await assert.rejects(validatePortableRpuArtifacts(paths), /canonical SemVer/);
    });
  }
  const rpu = rpuWith({ version: '1.9.1' });
  await withArtifact({ rpu }, async (paths) => {
    await assert.rejects(validatePortableRpuArtifacts(paths), /expected release context/);
  });
});

test('RPU rejects signed schema ranges outside the release schema contract', async () => {
  for (const overrides of [
    {
      minimum_schema: MINIMUM_PORTABLE_SCHEMA,
      maximum_schema: CURRENT_PORTABLE_SCHEMA - 1,
    },
    {
      minimum_schema: CURRENT_PORTABLE_SCHEMA,
      maximum_schema: CURRENT_PORTABLE_SCHEMA,
    },
    {
      minimum_schema: MINIMUM_PORTABLE_SCHEMA - 1,
      maximum_schema: CURRENT_PORTABLE_SCHEMA,
    },
    {
      minimum_schema: MINIMUM_PORTABLE_SCHEMA,
      maximum_schema: CURRENT_PORTABLE_SCHEMA + 1,
    },
  ]) {
    const rpu = rpuWith(overrides);
    await withArtifact({ rpu }, async (paths) => {
      await assert.rejects(
        validatePortableRpuArtifacts(paths),
        /does not authenticate its exact stored App image/,
      );
    });
  }
});

test('RPU rejects a mismatched supervisor capability or App session identity', async () => {
  for (const overrides of [
    { minimum_supervisor_protocol: PORTABLE_SUPERVISOR_CAPABILITY - 1 },
    { minimum_supervisor_protocol: PORTABLE_SUPERVISOR_CAPABILITY + 1 },
    { app_session_protocol: 'renderpilot-portable-app-session-v0' },
  ]) {
    const rpu = rpuWith(overrides);
    await withArtifact({ rpu }, async (paths) => {
      await assert.rejects(
        validatePortableRpuArtifacts({
          rawPath: paths.rawPath,
          rpuPath: paths.rpuPath,
          signaturePath: paths.signaturePath,
          zipPath: paths.zipPath,
          zipEntry: ZIP_ENTRY,
          expectedVersion: VERSION,
        }),
        /Portable RPU manifest does not authenticate its exact stored App image/,
      );
    });
  }
});

test('RPU rejects extra duplicate traversal and unauthenticated manifest content', async () => {
  const cases = [
    {
      rpu: rpuWith({}, [{ name: 'foreign.bin', contents: 'foreign' }]),
      pattern: /exactly (?:the expected|its canonical)/,
    },
    {
      rpu: rpuWith({}, [{ name: 'app/renderpilot-app.exe', contents: 'duplicate' }]),
      pattern: /duplicate|exactly (?:the expected|its canonical)/,
    },
    {
      rpu: rpuWith({}, [{ name: '../escape.exe', contents: 'escape' }]),
      pattern: /unsafe|canonical relative|exactly (?:the expected|its canonical)/,
    },
    { rpu: rpuWith({ app_sha256: '0'.repeat(64) }), pattern: /does not authenticate/ },
    { rpu: rpuWith({ platform: 'linux-x86_64' }), pattern: /does not authenticate/ },
    { rpu: rpuWith({ portable_role: 'supervisor' }), pattern: /does not authenticate/ },
  ];
  for (const { rpu, pattern } of cases) {
    await withArtifact({ rpu }, async (paths) => {
      await assert.rejects(validatePortableRpuArtifacts(paths), pattern);
    });
  }
});

test('RPU validation rejects raw/public and raw/ZIP identity splits', async () => {
  const rpu = rpuWith();
  await withArtifact({ publicRpu: rpuWith({ version: '1.9.1' }), rpu }, async (paths) => {
    await assert.rejects(validatePortableRpuArtifacts(paths), /does not embed the exact public/);
  });
  const raw = rawOverlay(rpu);
  await withArtifact({ raw, rpu, zipRaw: Buffer.from('wrong ZIP raw identity') }, async (paths) => {
    await assert.rejects(validatePortableRpuArtifacts(paths), /does not equal the signed raw/);
  });
});
