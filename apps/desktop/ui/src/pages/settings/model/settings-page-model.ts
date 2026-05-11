import type { ThemeMode } from '@shared/theme';
import type { SelectOption } from '@shared/ui';
import { type LanguageMode } from '@entities/settings';

export type ThemeModeHandler = (mode: ThemeMode) => void;
export type LanguageModeHandler = (mode: LanguageMode) => void;

export const themeOptions = [
  { value: 'system', label: 'System' },
  { value: 'dark', label: 'Dark' },
  { value: 'light', label: 'Light' },
] as const satisfies readonly SelectOption<ThemeMode>[];

export const languageOptions = [
  { value: 'system', label: 'Follow system' },
  { value: 'en', label: 'English' },
  { value: 'ru', label: 'Russian' },
] as const satisfies readonly SelectOption<LanguageMode>[];

export function isOptionValue<Value extends string>(
  value: string,
  options: readonly SelectOption<Value>[],
): value is Value {
  return options.some((option) => option.value === value);
}
