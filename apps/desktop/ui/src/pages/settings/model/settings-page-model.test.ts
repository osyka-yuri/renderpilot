import { describe, expect, it } from 'vitest';
import {
  isOptionValue,
  languageOptions,
  type SettingsSelectOption,
  tabOptions,
  themeOptions,
} from './settings-page-model';

type OptionValidationCase = {
  name: string;
  options: readonly SettingsSelectOption[];
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

function expectUniqueValues(values: readonly unknown[]): void {
  expect(new Set(values).size).toBe(values.length);
}

function expectNonEmptyString(value: string): void {
  expect(value.trim()).toBe(value);
  expect(value).not.toHaveLength(0);
}

describe('settings-page-model', () => {
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

  describe('tabOptions', () => {
    it('has exactly 2 tabs with non-empty labels and unique values', () => {
      expect(tabOptions.length).toBe(2);
      expectUniqueValues(tabOptions.map((t) => t.value));
      for (const tab of tabOptions) {
        expectNonEmptyString(tab.label);
      }
    });
  });
});
