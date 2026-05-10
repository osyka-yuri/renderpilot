import { describe, expect, it } from 'vitest';
import {
  coverSourceToggleRows,
  defaultCoverSourcesState,
  formatBooleanSetting,
  isOptionValue,
  languageOptions,
  themeOptions,
  type SelectOption,
} from '@features/settings/settings-screen-model';
import {
  COVERS_GOG_CDN_SETTING_KEY,
  COVERS_STEAM_CDN_SETTING_KEY,
  COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
} from '@shared/catalog/catalog-setting-keys';

type OptionValidationCase = {
  name: string;
  options: readonly SelectOption<string>[];
  validValues: readonly string[];
  invalidValues: readonly string[];
};

const optionValidationCases = [
  {
    name: 'theme',
    options: themeOptions,
    validValues: ['system', 'dark', 'light'],
    invalidValues: ['', 'neon', 'auto'],
  },
  {
    name: 'language',
    options: languageOptions,
    validValues: ['system', 'en', 'ru'],
    invalidValues: ['', 'de', 'english'],
  },
] as const satisfies readonly OptionValidationCase[];

const expectedCoverPolicyBySetting = {
  [COVERS_STEAM_CDN_SETTING_KEY]: 'steamCdn',
  [COVERS_GOG_CDN_SETTING_KEY]: 'gogCdn',
  [COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY]: 'steamgriddb',
} as const satisfies Record<string, keyof typeof defaultCoverSourcesState>;

function expectUniqueValues(values: readonly unknown[]): void {
  expect(new Set(values).size).toBe(values.length);
}

function expectNonEmptyString(value: string): void {
  expect(value.trim()).toBe(value);
  expect(value).not.toHaveLength(0);
}

describe('settings-screen-model', () => {
  describe('formatBooleanSetting', () => {
    it.each([
      [true, 'true'],
      [false, 'false'],
    ] as const)('formats %s for catalog persistence', (value, expected) => {
      expect(formatBooleanSetting(value)).toBe(expected);
    });
  });

  describe('isOptionValue', () => {
    it.each(optionValidationCases)(
      'accepts only known $name option values',
      ({ options, validValues, invalidValues }) => {
        for (const value of validValues) {
          expect(isOptionValue(value, options)).toBe(true);
        }

        for (const value of invalidValues) {
          expect(isOptionValue(value, options)).toBe(false);
        }
      },
    );

    it.each(optionValidationCases)('keeps $name option values unique', ({ options }) => {
      expectUniqueValues(options.map((option) => option.value));
    });

    it.each(optionValidationCases)('keeps $name option labels non-empty', ({ options }) => {
      for (const option of options) {
        expectNonEmptyString(option.label);
      }
    });
  });

  describe('coverSourceToggleRows', () => {
    it('declares one row for every supported cover source setting', () => {
      expect(coverSourceToggleRows).toHaveLength(Object.keys(expectedCoverPolicyBySetting).length);

      expectUniqueValues(coverSourceToggleRows.map((row) => row.settingKey));
    });

    it('binds each catalog setting key to the expected policy key', () => {
      const actualPolicyBySetting = Object.fromEntries(
        coverSourceToggleRows.map((row) => [row.settingKey, row.policyKey]),
      );

      expect(actualPolicyBySetting).toEqual(expectedCoverPolicyBySetting);
    });

    it('points every row to an existing default cover source policy', () => {
      for (const row of coverSourceToggleRows) {
        expect(defaultCoverSourcesState).toHaveProperty(row.policyKey);
        expect(typeof defaultCoverSourcesState[row.policyKey]).toBe('boolean');
      }
    });

    it('keeps cover source policy keys unique', () => {
      expectUniqueValues(coverSourceToggleRows.map((row) => row.policyKey));
    });

    it('provides complete non-empty UI metadata for every cover source row', () => {
      for (const row of coverSourceToggleRows) {
        expectNonEmptyString(row.ariaLabel);
        expectNonEmptyString(row.eyebrow);
        expectNonEmptyString(row.title);
        expectNonEmptyString(row.description);
      }
    });
  });
});
