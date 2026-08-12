import { createHash } from 'node:crypto';

export function fail(message) {
  throw new Error(message);
}

export function requireText(value, label) {
  if (typeof value !== 'string' || value.length === 0) {
    fail(`${label} must be a non-empty string.`);
  }
  return value;
}

export function requireObject(value, label) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    fail(`${label} must be an object.`);
  }
  return value;
}

export function sha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}
