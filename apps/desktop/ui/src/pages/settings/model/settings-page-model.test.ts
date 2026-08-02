import { describe, expect, it } from 'vitest';
import {
  createSettingsTabMemory,
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
    validValues: ['system', 'en', 'zh-Hans', 'zh-Hant'],
    invalidValues: ['', 'english', 'zh'],
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

    it.each(optionValidationCases)('keeps $name option label keys non-empty', ({ options }) => {
      for (const option of options) {
        expectNonEmptyString(option.labelKey);
      }
    });
  });

  describe('tabOptions', () => {
    it('has exactly 4 tabs with non-empty label keys and unique values', () => {
      expect(tabOptions.length).toBe(4);
      expectUniqueValues(tabOptions.map((t) => t.value));
      for (const tab of tabOptions) {
        expectNonEmptyString(tab.labelKey);
      }
    });
  });

  it('exposes every language in stable UI order', () => {
    expect(languageOptions).toEqual([
      { value: 'system', labelKey: 'settings.language.system' },
      { value: 'en', labelKey: 'settings.language.en' },
      { value: 'ru', labelKey: 'settings.language.ru' },
      { value: 'es', labelKey: 'settings.language.es' },
      { value: 'fr', labelKey: 'settings.language.fr' },
      { value: 'de', labelKey: 'settings.language.de' },
      { value: 'ja', labelKey: 'settings.language.ja' },
      { value: 'zh-Hans', labelKey: 'settings.language.zhHans' },
      { value: 'zh-Hant', labelKey: 'settings.language.zhHant' },
    ]);
  });

  describe('settings tab memory', () => {
    it('defaults to the general tab', () => {
      const memory = createSettingsTabMemory();

      expect(memory.getInitialTab()).toBe('general');
    });

    it('remembers a known tab value', () => {
      const memory = createSettingsTabMemory();

      memory.rememberTab('nvidia');
      expect(memory.getInitialTab()).toBe('nvidia');

      memory.rememberTab('catalog');
      expect(memory.getInitialTab()).toBe('catalog');
    });

    it('maps the legacy ReShade tab value to RenoDX', () => {
      const memory = createSettingsTabMemory();

      memory.rememberTab('reshade');

      expect(memory.getInitialTab()).toBe('renodx');
    });

    it('ignores unknown tab values', () => {
      const memory = createSettingsTabMemory();

      memory.rememberTab('catalog');
      memory.rememberTab('does-not-exist');

      expect(memory.getInitialTab()).toBe('catalog');
    });
  });
});
