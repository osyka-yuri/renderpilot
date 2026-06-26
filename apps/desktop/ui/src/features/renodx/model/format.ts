/**
 * Date formatting for the RenoDX card, keyed to the active UI locale.
 *
 * RenoDX add-ons are rolling snapshots with no version number, so the card
 * anchors "what's installed" to dates: the add-on's upstream `Last-Modified`
 * date, the local install date, and a relative "last checked" time.
 */
import { getLocale } from '@shared/i18n';

/**
 * Formats a Unix-epoch-ms timestamp as a short, localized absolute date
 * (e.g. "18 Jun 2026").
 */
export function formatDate(ms: number): string {
  return new Intl.DateTimeFormat(getLocale(), {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  }).format(new Date(ms));
}

/**
 * Formats a Unix-epoch-ms timestamp as a coarse, localized relative time
 * (e.g. "2 minutes ago"). The unit scales from seconds to days; older stamps are
 * shown in days.
 */
export function formatRelative(ms: number): string {
  const rtf = new Intl.RelativeTimeFormat(getLocale(), { numeric: 'auto' });
  const diffSeconds = Math.round((ms - Date.now()) / 1000);
  const abs = Math.abs(diffSeconds);
  let value: number;
  let unit: Intl.RelativeTimeFormatUnit;
  if (abs < 60) {
    value = diffSeconds;
    unit = 'second';
  } else if (abs < 3600) {
    value = Math.round(diffSeconds / 60);
    unit = 'minute';
  } else if (abs < 86_400) {
    value = Math.round(diffSeconds / 3600);
    unit = 'hour';
  } else {
    value = Math.round(diffSeconds / 86_400);
    unit = 'day';
  }
  return rtf.format(value, unit);
}

/**
 * Parses an HTTP `Last-Modified` date string (RFC 1123, which `Date.parse`
 * handles natively) and formats it as a short localized date, or returns `null`
 * for a missing/unparseable value so the caller can fall back to the install date.
 */
export function formatHttpDate(raw: string | null | undefined): string | null {
  if (!raw) return null;
  const ms = Date.parse(raw);
  return Number.isNaN(ms) ? null : formatDate(ms);
}
