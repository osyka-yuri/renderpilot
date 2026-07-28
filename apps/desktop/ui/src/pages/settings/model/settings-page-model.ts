import type { ThemeMode } from '@shared/theme';
import { type LanguageMode, type MessageKey } from '@shared/i18n';

export type SettingsSelectOption<Value extends string = string> = {
  value: Value;
  labelKey: MessageKey;
  disabled?: boolean;
};

export type ThemeModeHandler = (mode: ThemeMode) => void;
export type LanguageModeHandler = (mode: LanguageMode) => Promise<void>;

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
  { value: 'renodx', labelKey: 'settings.tabs.renodx' },
  { value: 'catalog', labelKey: 'settings.tabs.catalog' },
  { value: 'nvidia', labelKey: 'settings.tabs.nvidia' },
] as const satisfies readonly SettingsTabOption[];

export type SettingsTabValue = (typeof tabOptions)[number]['value'];

/** In-session memory for the last active settings tab. */
export type SettingsTabMemory = {
  getInitialTab: () => SettingsTabValue;
  rememberTab: (value: string) => void;
};

function normalizeTabValue(value: string): SettingsTabValue | null {
  if (value === 'reshade') {
    return 'renodx';
  }
  return tabOptions.find((tab) => tab.value === value)?.value ?? null;
}

/**
 * Creates a self-contained settings tab memory.
 *
 * The singleton returned by {@link settingsTabMemory} is used by the page so
 * the active tab survives leaving and returning to the settings screen within
 * a session. Each call creates an isolated instance, which keeps tests free of
 * module-level mutable state.
 */
export function createSettingsTabMemory(): SettingsTabMemory {
  let rememberedTab: SettingsTabValue = 'general';

  return {
    getInitialTab() {
      return rememberedTab;
    },
    rememberTab(value: string) {
      const normalized = normalizeTabValue(value);
      if (normalized) {
        rememberedTab = normalized;
      }
    },
  };
}

/**
 * Shared in-session tab memory. Not persisted to storage — it resets on app
 * restart.
 */
export const settingsTabMemory = createSettingsTabMemory();

export function isOptionValue<Value extends string>(
  value: string,
  options: readonly SettingsSelectOption<Value>[],
): value is Value {
  return options.some((option) => option.value === value);
}
