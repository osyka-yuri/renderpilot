import type { ThemeMode } from '@shared/theme';
import { type LanguageMode } from '@entities/settings';

export type SettingsSelectOption<Value extends string = string> = {
  value: Value;
  label: string;
  disabled?: boolean;
};

export type ThemeModeHandler = (mode: ThemeMode) => void;
export type LanguageModeHandler = (mode: LanguageMode) => void;

export const themeOptions = [
  { value: 'system', label: 'System' },
  { value: 'dark', label: 'Dark' },
  { value: 'light', label: 'Light' },
] as const satisfies readonly SettingsSelectOption<ThemeMode>[];

export const languageOptions = [
  { value: 'system', label: 'Follow system' },
  { value: 'en', label: 'English' },
  { value: 'ru', label: 'Russian' },
] as const satisfies readonly SettingsSelectOption<LanguageMode>[];

export type SettingsTabOption = {
  value: string;
  label: string;
};

export const tabOptions = [
  { value: 'appearance', label: 'Appearance' },
  { value: 'catalog', label: 'Catalog' },
] as const satisfies readonly SettingsTabOption[];

export function isOptionValue<Value extends string>(
  value: string,
  options: readonly SettingsSelectOption<Value>[],
): value is Value {
  return options.some((option) => option.value === value);
}
