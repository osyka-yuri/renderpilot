import { isNumber, isString } from './guards';

export function requireString(value: unknown, fieldName: string): string {
  if (!isString(value)) {
    throw new TypeError(`${fieldName} must be a string.`);
  }

  return value;
}

export function requireNonBlankString(value: unknown, fieldName: string): string {
  const stringValue = requireString(value, fieldName);

  if (stringValue.trim().length === 0) {
    throw new TypeError(`${fieldName} must be a non-empty string.`);
  }

  return stringValue;
}

export function requireValidTimestampMs(value: unknown, fieldName: string): number {
  if (!isNumber(value) || !Number.isFinite(value) || value < 0) {
    throw new TypeError(`${fieldName} must be a finite non-negative number.`);
  }

  return value;
}
