import { isPlainObject } from './guards';

export function safeJsonParse(value: string): unknown {
  try {
    return JSON.parse(value);
  } catch {
    return null;
  }
}

export function isUnknownRecord(value: unknown): value is Record<string, unknown> {
  return isPlainObject(value);
}
