import type { ThemeMode } from '@shared/theme';
import { type LanguageMode, type MessageKey } from '@shared/i18n';

export type SettingsSelectOption<Value extends string = string> = {
  value: Value;
  labelKey: MessageKey;
  disabled?: boolean;
};

export type ThemeModeHandler = (mode: ThemeMode) => void;
export type LanguageModeHandler = (mode: LanguageMode) => void;

export const themeOptions = [
  { value: 'system', labelKey: 'settings.theme.system' },
  { value: 'dark', labelKey: 'settings.theme.dark' },
  { value: 'light', labelKey: 'settings.theme.light' },
] as const satisfies readonly SettingsSelectOption<ThemeMode>[];

export const languageOptions = [
  { value: 'system', labelKey: 'settings.language.system' },
  { value: 'en', labelKey: 'settings.language.en' },
  { value: 'ru', labelKey: 'settings.language.ru' },
  { value: 'es', labelKey: 'settings.language.es' },
  { value: 'zh', labelKey: 'settings.language.zh' },
  { value: 'fr', labelKey: 'settings.language.fr' },
  { value: 'de', labelKey: 'settings.language.de' },
  { value: 'ja', labelKey: 'settings.language.ja' },
] as const satisfies readonly SettingsSelectOption<LanguageMode>[];

export type SettingsTabOption = {
  value: string;
  labelKey: MessageKey;
};

export const tabOptions = [
  { value: 'general', labelKey: 'settings.tabs.general' },
  { value: 'catalog', labelKey: 'settings.tabs.catalog' },
] as const satisfies readonly SettingsTabOption[];

export function isOptionValue<Value extends string>(
  value: string,
  options: readonly SettingsSelectOption<Value>[],
): value is Value {
  return options.some((option) => option.value === value);
}
