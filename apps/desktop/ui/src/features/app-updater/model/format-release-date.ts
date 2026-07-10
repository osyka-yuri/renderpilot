/**
 * Normalize optional updater release dates.
 *
 * Accepts only parseable dates and returns a stable ISO-like string, or null.
 * Never throws on malformed metadata.
 */

function parseOptionalDate(
  value: string | null | undefined,
): { date: Date; trimmed: string } | null {
  if (value == null) {
    return null;
  }

  const trimmed = value.trim();
  if (trimmed.length === 0) {
    return null;
  }

  const date = new Date(trimmed);
  if (Number.isNaN(date.getTime())) {
    return null;
  }

  return { date, trimmed };
}

export function normalizeReleaseDate(value: string | null | undefined): string | null {
  const parsed = parseOptionalDate(value);
  if (!parsed) {
    return null;
  }

  // Prefer the original if it already looks like a clean RFC 3339/ISO string;
  // otherwise fall back to ISO UTC.
  if (/^\d{4}-\d{2}-\d{2}/.test(parsed.trimmed)) {
    return parsed.trimmed;
  }

  return parsed.date.toISOString();
}

/**
 * Format a normalized release date for display in the current locale.
 * Returns null when the value is absent or invalid.
 */
export function formatReleaseDateForLocale(
  value: string | null | undefined,
  locale: string,
): string | null {
  const parsed = parseOptionalDate(value);
  if (!parsed) {
    return null;
  }

  try {
    return new Intl.DateTimeFormat(locale, {
      year: 'numeric',
      month: 'long',
      day: 'numeric',
    }).format(parsed.date);
  } catch {
    return null;
  }
}
