import { readFile, writeFile } from 'node:fs/promises';
import process from 'node:process';

import { fail, sha256 } from './release-manifest-common.mjs';
import { extractExactZipEntries, extractZipEntry } from './release-manifest-zip.mjs';
import { validateTauriSignature } from './release-signature.mjs';

export const RPSX1_MAGIC = Buffer.from('RPSX1', 'ascii');
export const RPSX1_VERSION = 1;
export const RPSX1_FOOTER_BYTES = 102;
const CANONICAL_SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

export function requireCanonicalSemVer(version, label) {
  if (typeof version !== 'string' || !CANONICAL_SEMVER.test(version)) {
    fail(`${label} must be nonempty canonical SemVer.`);
  }
  return version;
}

function assertRange(start, length, limit, label) {
  if (
    !Number.isSafeInteger(start) ||
    !Number.isSafeInteger(length) ||
    start < 0 ||
    length < 0 ||
    start > limit - length
  ) {
    fail(`${label} is outside the RPSX1 payload range.`);
  }
}

function readSafeU64(buffer, offset, label) {
  const value = buffer.readBigUInt64LE(offset);
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
    fail(`${label} exceeds JavaScript's exact integer range.`);
  }
  return Number(value);
}

/** Builds the fixed raw overlay: [supervisor][public .rpu][public .rpu.sig][RPSX1]. */
export function assembleRpsx1({ rpu, signature, supervisor, expectedVersion }) {
  validatePortableRpuBytes(rpu, expectedVersion);
  const signatureText = signature.toString('utf8');
  if (Buffer.from(signatureText, 'utf8').compare(signature) !== 0) {
    fail('Portable RPU signature must be exact UTF-8 text.');
  }
  validateTauriSignature(signatureText, 'Portable RPU signature');
  const rpuOffset = supervisor.length;
  const signatureOffset = rpuOffset + rpu.length;
  const footer = Buffer.alloc(RPSX1_FOOTER_BYTES);
  RPSX1_MAGIC.copy(footer, 0);
  footer.writeUInt8(RPSX1_VERSION, 5);
  footer.writeBigUInt64LE(BigInt(rpuOffset), 6);
  footer.writeBigUInt64LE(BigInt(rpu.length), 14);
  footer.writeBigUInt64LE(BigInt(signatureOffset), 22);
  footer.writeBigUInt64LE(BigInt(signature.length), 30);
  Buffer.from(sha256(rpu), 'hex').copy(footer, 38);
  Buffer.from(sha256(signature), 'hex').copy(footer, 70);
  return Buffer.concat([supervisor, rpu, signature, footer]);
}

/** Parses and authenticates the exact public RPU/signature bytes in a raw SFX. */
export function parseRpsx1(raw) {
  if (raw.length < RPSX1_FOOTER_BYTES) {
    fail('Raw portable supervisor is smaller than its RPSX1 footer.');
  }
  const footer = raw.subarray(raw.length - RPSX1_FOOTER_BYTES);
  if (!footer.subarray(0, 5).equals(RPSX1_MAGIC) || footer.readUInt8(5) !== RPSX1_VERSION) {
    fail('Raw portable supervisor has no supported RPSX1 footer.');
  }
  const bodyEnd = raw.length - RPSX1_FOOTER_BYTES;
  const rpuOffset = readSafeU64(footer, 6, 'RPU offset');
  const rpuLength = readSafeU64(footer, 14, 'RPU length');
  const signatureOffset = readSafeU64(footer, 22, 'RPU signature offset');
  const signatureLength = readSafeU64(footer, 30, 'RPU signature length');
  assertRange(rpuOffset, rpuLength, bodyEnd, 'RPU');
  assertRange(signatureOffset, signatureLength, bodyEnd, 'RPU signature');
  if (rpuOffset + rpuLength !== signatureOffset || signatureOffset + signatureLength !== bodyEnd) {
    fail('RPSX1 must contain adjacent public RPU and signature bytes at the raw overlay tail.');
  }
  const rpu = raw.subarray(rpuOffset, rpuOffset + rpuLength);
  const signature = raw.subarray(signatureOffset, signatureOffset + signatureLength);
  if (
    !footer.subarray(38, 70).equals(Buffer.from(sha256(rpu), 'hex')) ||
    !footer.subarray(70, 102).equals(Buffer.from(sha256(signature), 'hex'))
  ) {
    fail('RPSX1 digest does not match its embedded public bytes.');
  }
  validateTauriSignature(signature.toString('utf8'), 'Embedded portable RPU signature');
  return { rpu, signature, supervisor: raw.subarray(0, rpuOffset) };
}

export async function validatePortableRpuArtifacts({
  rawPath,
  rpuPath,
  signaturePath,
  zipPath,
  zipEntry,
  expectedVersion,
}) {
  const [raw, publicRpu, publicSignature, zip] = await Promise.all([
    readFile(rawPath),
    readFile(rpuPath),
    readFile(signaturePath),
    readFile(zipPath),
  ]);
  const embedded = parseRpsx1(raw);
  if (!embedded.rpu.equals(publicRpu) || !embedded.signature.equals(publicSignature)) {
    fail('Raw RPSX1 overlay does not embed the exact public RPU and signature bytes.');
  }
  const zipRaw = extractZipEntry(zip, zipEntry);
  if (!zipRaw.equals(raw)) {
    fail(`Portable ZIP entry ${zipEntry} does not equal the signed raw portable supervisor bytes.`);
  }
  const manifest = validatePortableRpuBytes(publicRpu, expectedVersion);
  return {
    rawSha256: sha256(raw),
    rpuSha256: sha256(publicRpu),
    signatureSha256: sha256(publicSignature),
    version: manifest.version,
  };
}

function validatePortableRpuBytes(rpu, expectedVersion) {
  const rpuEntries = extractExactZipEntries(rpu, ['rpu-manifest.json', 'app/renderpilot-app.exe']);
  let manifest;
  try {
    manifest = JSON.parse(rpuEntries.get('rpu-manifest.json').toString('utf8'));
  } catch (error) {
    fail(`Portable RPU manifest is invalid JSON: ${error.message}`);
  }
  const app = rpuEntries.get('app/renderpilot-app.exe');
  const expectedFields = [
    'protocol',
    'platform',
    'version',
    'app_sha256',
    'app_length',
    'minimum_supervisor_protocol',
    'minimum_schema',
    'maximum_schema',
    'portable_role',
  ];
  if (
    !manifest ||
    typeof manifest !== 'object' ||
    Array.isArray(manifest) ||
    Object.keys(manifest).length !== expectedFields.length ||
    expectedFields.some((field) => !(field in manifest)) ||
    manifest.protocol !== 'renderpilot-portable-rpu-v1' ||
    manifest.platform !== 'windows-x86_64-portable' ||
    manifest.portable_role !== 'app' ||
    !Number.isInteger(manifest.minimum_supervisor_protocol) ||
    manifest.minimum_supervisor_protocol > 1 ||
    !Number.isInteger(manifest.minimum_schema) ||
    !Number.isInteger(manifest.maximum_schema) ||
    manifest.minimum_schema !== 4 ||
    manifest.maximum_schema !== 16 ||
    manifest.app_length !== app.length ||
    manifest.app_sha256 !== sha256(app)
  ) {
    fail('Portable RPU manifest does not authenticate its exact stored App image.');
  }
  requireCanonicalSemVer(manifest.version, 'Portable RPU manifest version');
  if (
    expectedVersion !== undefined &&
    manifest.version !== requireCanonicalSemVer(expectedVersion, 'Expected portable RPU version')
  ) {
    fail('Portable RPU manifest version did not match its expected release context.');
  }
  return manifest;
}

function parseOptions(args) {
  const options = new Map();
  for (let index = 0; index < args.length; index += 2) {
    const key = args[index];
    const value = args[index + 1];
    if (!key?.startsWith('--') || value === undefined || options.has(key)) {
      fail('Portable RPU command requires one value for every distinct option.');
    }
    options.set(key, value);
  }
  return (key) => {
    const value = options.get(key);
    if (!value) {
      fail(`${key} is required.`);
    }
    return value;
  };
}

async function main(args) {
  const [command, ...rest] = args;
  const option = parseOptions(rest);
  if (command === 'assemble') {
    const output = option('--output');
    const [supervisor, rpu, signature] = await Promise.all([
      readFile(option('--supervisor')),
      readFile(option('--rpu')),
      readFile(option('--signature')),
    ]);
    await writeFile(
      output,
      assembleRpsx1({
        supervisor,
        rpu,
        signature,
        expectedVersion: option('--expected-version'),
      }),
      { flag: 'wx' },
    );
    return;
  }
  if (command === 'validate') {
    const result = await validatePortableRpuArtifacts({
      rawPath: option('--raw'),
      rpuPath: option('--rpu'),
      signaturePath: option('--signature'),
      zipPath: option('--zip'),
      zipEntry: option('--zip-entry'),
      expectedVersion: option('--expected-version'),
    });
    process.stdout.write(`${JSON.stringify(result)}\n`);
    return;
  }
  fail('Portable RPU command must be assemble or validate.');
}

if (import.meta.url === `file:///${process.argv[1]?.replaceAll('\\', '/')}`) {
  main(process.argv.slice(2)).catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
