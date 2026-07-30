import type { Locale } from '@shared/i18n';
import { createNumberFormatter } from '@shared/intl';

const integerPercentFormatter = createNumberFormatter({
  style: 'percent',
  maximumFractionDigits: 0,
});

export function formatPercent(ratio: number, locale: Locale): string {
  const normalizedRatio = Number.isFinite(ratio) ? clamp(ratio, 0, 1) : 0;
  return integerPercentFormatter(locale).format(normalizedRatio);
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
