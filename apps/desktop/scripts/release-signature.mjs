import { fail, requireText } from './release-manifest-common.mjs';

/** Validates the canonical base64-encoded Minisign text produced by Tauri. */
export function validateTauriSignature(signature, label = 'Updater signature') {
  const encoded = requireText(signature, label).trim();
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(encoded) || encoded.length % 4 !== 0) {
    fail(`${label} must be canonical base64.`);
  }

  const decodedBytes = Buffer.from(encoded, 'base64');
  if (decodedBytes.toString('base64') !== encoded) {
    fail(`${label} must be canonical base64.`);
  }
  let decoded;
  try {
    decoded = new TextDecoder('utf-8', { fatal: true }).decode(decodedBytes);
  } catch {
    fail(`${label} must decode to UTF-8 Minisign text.`);
  }
  if (!decoded.startsWith('untrusted comment:') || !decoded.includes('\ntrusted comment:')) {
    fail(`${label} is not a Tauri minisign signature.`);
  }
  return encoded;
}
