import { isString } from '@shared/validation';

export function trimToEmpty(value: string | null | undefined): string {
  return typeof value === 'string' ? value.trim() : '';
}

export function trimToOptional(value: string | null | undefined): string | undefined {
  const trimmed = trimToEmpty(value);
  return trimmed.length > 0 ? trimmed : undefined;
}

export function normalizeUniqueTrimmedStrings(values: readonly string[]): string[] {
  const seen = new Set<string>();
  const normalized: string[] = [];

  for (const rawValue of values) {
    const value = trimToEmpty(rawValue);
    if (value.length === 0 || seen.has(value)) {
      continue;
    }

    seen.add(value);
    normalized.push(value);
  }

  return normalized;
}

export function normalizeUniqueTrimmedStringsFromUnknown(values: readonly unknown[]): string[] {
  return normalizeUniqueTrimmedStrings(values.filter(isString));
}
