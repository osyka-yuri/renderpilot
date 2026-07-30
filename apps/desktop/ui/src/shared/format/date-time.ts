import {
  createDateTimeFormatter,
  createRelativeTimeFormatter,
  type IntlFormatterProvider,
} from '@shared/intl';
import type { Locale } from '@shared/i18n';

const MAX_DATE_TIMESTAMP = 8_640_000_000_000_000;
const SECONDS_PER_MINUTE = 60;
const SECONDS_PER_HOUR = 3_600;
const SECONDS_PER_DAY = 86_400;

const localShortDateFormatter = createDateTimeFormatter({
  day: 'numeric',
  month: 'short',
  year: 'numeric',
});

const localDateTimeFormatter = createDateTimeFormatter({
  dateStyle: 'medium',
  timeStyle: 'short',
});

const utcShortDateFormatter = createDateTimeFormatter({
  day: 'numeric',
  month: 'short',
  year: 'numeric',
  timeZone: 'UTC',
});

const utcLongDateFormatter = createDateTimeFormatter({
  day: 'numeric',
  month: 'long',
  year: 'numeric',
  timeZone: 'UTC',
});

const utcNumericDateFormatter = createDateTimeFormatter({
  day: '2-digit',
  month: '2-digit',
  year: 'numeric',
  timeZone: 'UTC',
});

const relativeTimeFormatter = createRelativeTimeFormatter({ numeric: 'auto' });

export function formatLocalShortDate(timestamp: number, locale: Locale): string | null {
  return formatDate(timestamp, locale, localShortDateFormatter);
}

export function formatLocalDateTime(timestamp: number, locale: Locale): string | null {
  return formatDate(timestamp, locale, localDateTimeFormatter);
}

export function formatUtcShortDate(timestamp: number, locale: Locale): string | null {
  return formatDate(timestamp, locale, utcShortDateFormatter);
}

export function formatUtcLongDate(timestamp: number, locale: Locale): string | null {
  return formatDate(timestamp, locale, utcLongDateFormatter);
}

export function formatUtcNumericDate(timestamp: number, locale: Locale): string | null {
  return formatDate(timestamp, locale, utcNumericDateFormatter);
}

export function formatRelativeTime(
  timestamp: number,
  locale: Locale,
  now = Date.now(),
): string | null {
  if (!isValidTimestamp(timestamp) || !isValidTimestamp(now)) {
    return null;
  }

  const diffSeconds = Math.round((timestamp - now) / 1000);
  const absoluteSeconds = Math.abs(diffSeconds);
  const formatter = relativeTimeFormatter(locale);

  if (absoluteSeconds < SECONDS_PER_MINUTE) {
    return formatter.format(diffSeconds, 'second');
  }
  if (absoluteSeconds < SECONDS_PER_HOUR) {
    return formatter.format(Math.round(diffSeconds / SECONDS_PER_MINUTE), 'minute');
  }
  if (absoluteSeconds < SECONDS_PER_DAY) {
    return formatter.format(Math.round(diffSeconds / SECONDS_PER_HOUR), 'hour');
  }

  return formatter.format(Math.round(diffSeconds / SECONDS_PER_DAY), 'day');
}

function formatDate(
  timestamp: number,
  locale: Locale,
  formatter: IntlFormatterProvider<Intl.DateTimeFormat>,
): string | null {
  return isValidTimestamp(timestamp) ? formatter(locale).format(timestamp) : null;
}

function isValidTimestamp(value: number): boolean {
  return Number.isFinite(value) && Math.abs(value) <= MAX_DATE_TIMESTAMP;
}
