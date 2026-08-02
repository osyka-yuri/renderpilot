import { describe, expect, it } from 'vitest';

import { decodeStoredLanguageMode, encodeStoredLanguageMode } from './language-mode-codec';

describe('language mode codec v2', () => {
  it.each(['system', 'en', 'ru', 'es', 'fr', 'de', 'ja', 'zh-Hans', 'zh-Hant'] as const)(
    'round-trips %s',
    (mode) => {
      const encoded = encodeStoredLanguageMode(mode);
      expect(encoded).toBe(`{"version":2,"mode":"${mode}"}`);
      expect(decodeStoredLanguageMode(encoded)).toEqual({ mode, migrate: false });
    },
  );

  it.each([
    ['system', 'system'],
    ['en', 'en'],
    ['ru', 'ru'],
    ['es', 'es'],
    ['fr', 'fr'],
    ['de', 'de'],
    ['ja', 'ja'],
    ['zh', 'zh-Hans'],
  ] as const)('decodes legacy %s as %s and requests migration', (legacy, expected) => {
    expect(decodeStoredLanguageMode(legacy)).toEqual({ mode: expected, migrate: true });
  });

  it.each([
    null,
    '',
    '{',
    '"ru"',
    'null',
    '[]',
    '{"version":1,"mode":"ru"}',
    '{"version":3,"mode":"ru"}',
    '{"version":2,"mode":"zh"}',
    '{"version":2,"mode":42}',
  ])('uses system for corrupt or unsupported input: %j', (raw) => {
    expect(decodeStoredLanguageMode(raw)).toEqual({ mode: 'system', migrate: false });
  });

  it('accepts additive v2 fields', () => {
    expect(decodeStoredLanguageMode('{"version":2,"mode":"fr","source":"settings"}')).toEqual({
      mode: 'fr',
      migrate: false,
    });
  });
});
