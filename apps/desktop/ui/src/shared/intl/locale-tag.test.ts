import { describe, expect, it } from 'vitest';

import { parseLocaleTag } from './locale-tag';

describe('parseLocaleTag', () => {
  it('canonicalizes and decomposes BCP 47 tags once', () => {
    expect(parseLocaleTag('ZH-hant-tw')).toEqual({
      tag: 'zh-Hant-TW',
      language: 'zh',
      script: 'Hant',
      region: 'TW',
    });
    expect(parseLocaleTag('ZH-hant-tw')).toBe(parseLocaleTag('zh-Hant-TW'));
  });

  it('preserves the platform RangeError contract for invalid tags', () => {
    expect(() => parseLocaleTag('not_a_tag')).toThrow(RangeError);
  });
});
