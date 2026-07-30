import { parseAbsolute } from '@internationalized/date';
import { trimToOptional } from '@shared/text';

const RFC_3339_ABSOLUTE_PATTERN =
  /^\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:[Zz]|[+-]\d{2}:\d{2})$/u;

const IMF_FIXDATE_PATTERN =
  /^(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun), \d{2} (?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec) \d{4} \d{2}:\d{2}:\d{2} GMT$/u;

/**
 * Parses an absolute RFC 3339 timestamp into epoch milliseconds.
 *
 * Date-only values, timestamps without an offset and calendar overflows are
 * rejected instead of inheriting the host-dependent behavior of `Date.parse`.
 */
export function parseRfc3339Timestamp(value: string | null | undefined): number | null {
  const trimmed = trimToOptional(value);
  if (trimmed === undefined || !RFC_3339_ABSOLUTE_PATTERN.test(trimmed)) {
    return null;
  }

  const normalizedSeparator =
    trimmed[10] === 't' ? `${trimmed.slice(0, 10)}T${trimmed.slice(11)}` : trimmed;
  const normalized = normalizedSeparator.endsWith('z')
    ? `${normalizedSeparator.slice(0, -1)}Z`
    : normalizedSeparator;

  try {
    const timestamp = parseAbsolute(normalized, 'UTC').toDate().getTime();
    return Number.isFinite(timestamp) ? timestamp : null;
  } catch {
    return null;
  }
}

/**
 * Parses the canonical HTTP-date representation (IMF-fixdate).
 *
 * The round-trip check validates the weekday as well as every calendar/time
 * field and deliberately rejects obsolete RFC 850/asctime representations.
 */
export function parseHttpDateTimestamp(value: string | null | undefined): number | null {
  const trimmed = trimToOptional(value);
  if (trimmed === undefined || !IMF_FIXDATE_PATTERN.test(trimmed)) {
    return null;
  }

  const timestamp = Date.parse(trimmed);
  if (!Number.isFinite(timestamp)) {
    return null;
  }

  return new Date(timestamp).toUTCString() === trimmed ? timestamp : null;
}
