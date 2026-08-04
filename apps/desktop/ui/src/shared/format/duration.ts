import { createDurationFormatter } from '@shared/intl';
import type { Locale } from '@shared/i18n';

const SECONDS_PER_MINUTE = 60;
const SECONDS_PER_HOUR = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY = 24 * SECONDS_PER_HOUR;

const compactDurationFormatter = createDurationFormatter({
  style: 'narrow',
  secondsDisplay: 'always',
});

export function formatCompactDurationSeconds(seconds: number, locale: Locale): string | null {
  if (!Number.isSafeInteger(seconds) || seconds < 0) {
    return null;
  }

  const days = Math.floor(seconds / SECONDS_PER_DAY);
  const afterDays = seconds % SECONDS_PER_DAY;
  const hours = Math.floor(afterDays / SECONDS_PER_HOUR);
  const afterHours = afterDays % SECONDS_PER_HOUR;
  const minutes = Math.floor(afterHours / SECONDS_PER_MINUTE);

  return compactDurationFormatter(locale).format({
    days,
    hours,
    minutes,
    seconds: afterHours % SECONDS_PER_MINUTE,
  });
}
