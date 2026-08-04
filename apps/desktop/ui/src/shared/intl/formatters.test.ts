import { describe, expect, it } from 'vitest';

import {
  createDateTimeFormatter,
  createDurationFormatter,
  createListFormatter,
  createNumberFormatter,
  createPluralRules,
  createRelativeTimeFormatter,
  createSegmenter,
} from './formatters';

describe('Intl formatter providers', () => {
  it('reuses a formatter by canonical locale', () => {
    const formatter = createNumberFormatter({ style: 'percent' });

    expect(formatter('EN-us')).toBe(formatter('en-US'));
  });

  it('keeps providers and locales isolated', () => {
    const integer = createNumberFormatter({ maximumFractionDigits: 0 });
    const decimal = createNumberFormatter({ maximumFractionDigits: 1 });

    expect(integer('en')).not.toBe(decimal('en'));
    expect(integer('en')).not.toBe(integer('fr'));
  });

  it('snapshots options when the provider is defined', () => {
    const options: Intl.NumberFormatOptions = { maximumFractionDigits: 0 };
    const formatter = createNumberFormatter(options);

    options.maximumFractionDigits = 3;

    expect(formatter('en').resolvedOptions().maximumFractionDigits).toBe(0);
  });

  it('supports every formatter family currently used by shared code', () => {
    const dateTime = createDateTimeFormatter({ timeZone: 'UTC' });
    const relativeTime = createRelativeTimeFormatter({ numeric: 'auto' });
    const list = createListFormatter({ type: 'conjunction' });
    const duration = createDurationFormatter({ style: 'narrow' });
    const pluralRules = createPluralRules({ type: 'cardinal' });
    const segmenter = createSegmenter({ granularity: 'grapheme' });

    expect(dateTime('fr')).toBe(dateTime('fr'));
    expect(relativeTime('ru')).toBe(relativeTime('ru'));
    expect(list('ja')).toBe(list('ja'));
    expect(duration('en')).toBe(duration('en'));
    expect(pluralRules('de')).toBe(pluralRules('de'));
    expect(segmenter('ZH-hans')).toBe(segmenter('zh-Hans'));
  });
});
