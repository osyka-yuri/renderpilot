import type { Locale } from '@shared/i18n';
import { createSegmenter } from '@shared/intl';

const graphemeSegmenter = createSegmenter({ granularity: 'grapheme' });

export function takeGraphemePrefix(value: string, count: number, locale: Locale): string {
  if (!Number.isSafeInteger(count) || count <= 0) {
    return '';
  }

  let prefix = '';
  let remaining = count;
  for (const { segment } of graphemeSegmenter(locale).segment(value)) {
    prefix += segment;
    remaining -= 1;
    if (remaining === 0) {
      break;
    }
  }

  return prefix;
}
