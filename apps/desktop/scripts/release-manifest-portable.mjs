import { readFile } from 'node:fs/promises';

import { sha256, fail } from './release-manifest-common.mjs';
import { validatePortableRpuArtifacts } from './portable-rpu.mjs';
import { validateTauriSignature } from './release-signature.mjs';

export async function validatePortableArtifacts({
  rawPath,
  rawSignaturePath,
  rpuPath,
  rpuSignaturePath,
  expectedVersion,
  zipEntry,
  zipPath,
}) {
  const [raw, rawSignatureText, rpuSignatureText, identity] = await Promise.all([
    readFile(rawPath),
    readFile(rawSignaturePath, 'utf8'),
    readFile(rpuSignaturePath, 'utf8'),
    validatePortableRpuArtifacts({
      rawPath,
      rpuPath,
      signaturePath: rpuSignaturePath,
      zipPath,
      zipEntry,
      expectedVersion,
    }),
  ]);
  if (raw.length === 0) {
    fail('Raw portable supervisor must not be empty.');
  }
  const rawSignature = validateTauriSignature(
    rawSignatureText,
    'Raw portable supervisor signature',
  );
  const rpuSignature = validateTauriSignature(rpuSignatureText, 'Portable RPU signature');

  return {
    rawSha256: sha256(raw),
    rawSignature,
    rpuSignature,
    ...identity,
  };
}
