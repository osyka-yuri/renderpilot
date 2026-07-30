import type { Locale } from '@shared/i18n';
import { createNumberFormatter } from '@shared/intl';

const BYTES_PER_UNIT = 1024;
const BYTE_FORMATTERS = [
  createByteFormatter('byte'),
  createByteFormatter('kilobyte'),
  createByteFormatter('megabyte'),
  createByteFormatter('gigabyte'),
  createByteFormatter('terabyte'),
] as const;

export function formatBytes(bytes: number, locale: Locale): string {
  const normalizedBytes = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;

  const unitIndex = Math.min(
    Math.max(
      normalizedBytes === 0 ? 0 : Math.floor(Math.log(normalizedBytes) / Math.log(BYTES_PER_UNIT)),
      0,
    ),
    BYTE_FORMATTERS.length - 1,
  );

  const value = normalizedBytes / BYTES_PER_UNIT ** unitIndex;
  return BYTE_FORMATTERS[unitIndex](locale).format(value);
}

function createByteFormatter(unit: Intl.NumberFormatOptions['unit']) {
  return createNumberFormatter({
    style: 'unit',
    unit,
    unitDisplay: 'short',
    maximumFractionDigits: 1,
  });
}
