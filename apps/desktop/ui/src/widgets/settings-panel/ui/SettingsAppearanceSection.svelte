<script lang="ts">
  import { Select, type SelectOption } from '@shared/ui';
  import SettingsSectionShell from './SettingsSectionShell.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingCopy from './SettingCopy.svelte';
  import SettingLabel from './SettingLabel.svelte';
  import type { ThemeMode } from '@shared/theme';
  import type { LanguageMode } from '@entities/settings';

  type SelectChangeHandler<TValue extends string> = (value: TValue) => void;

  const noop = () => undefined;

  type Props = {
    themeMode?: ThemeMode;
    languageMode?: LanguageMode;
    themeOptions?: readonly SelectOption<ThemeMode>[];
    languageOptions?: readonly SelectOption<LanguageMode>[];
    onThemeChange?: SelectChangeHandler<ThemeMode>;
    onLanguageChange?: SelectChangeHandler<LanguageMode>;
  };

  let {
    themeMode = 'system',
    languageMode = 'system',
    themeOptions = [],
    languageOptions = [],
    onThemeChange = noop,
    onLanguageChange = noop,
  }: Props = $props();

  function isSelectOptionValue<TValue extends string>(
    options: readonly SelectOption<TValue>[],
    value: string,
  ): value is TValue {
    return options.some((option) => option.value === value);
  }

  function handleThemeChange(value: string): void {
    if (!isSelectOptionValue(themeOptions, value)) {
      return;
    }

    onThemeChange(value);
  }

  function handleLanguageChange(value: string): void {
    if (!isSelectOptionValue(languageOptions, value)) {
      return;
    }

    onLanguageChange(value);
  }
</script>

<SettingsSectionShell
  titleId="appearance-title"
  eyebrow="Interface"
  title="Appearance and language"
  description="Keep the shell visually consistent across themes and languages without turning preferences into oversized cards."
>
  <div class="settings-list">
    <SettingRow>
      <SettingCopy>
        <SettingLabel>Display</SettingLabel>
        <h4>Theme</h4>
        <p>
          Follow the operating system appearance or choose a fixed theme while keeping the
          application palette internally consistent.
        </p>
      </SettingCopy>

      <span class="setting-control">
        <Select
          aria-label="Theme mode"
          options={themeOptions}
          value={themeMode}
          onValueChange={handleThemeChange}
        />
      </span>
    </SettingRow>

    <SettingRow>
      <SettingCopy>
        <SettingLabel>Localization</SettingLabel>
        <h4>Language</h4>
        <p>
          Use a scalable selector so more interface languages can be added later without changing
          the page structure.
        </p>
      </SettingCopy>

      <span class="setting-control">
        <Select
          aria-label="Interface language"
          options={languageOptions}
          value={languageMode}
          onValueChange={handleLanguageChange}
        />
      </span>
    </SettingRow>
  </div>
</SettingsSectionShell>

<style>
  .settings-list {
    display: grid;
    gap: 1rem;
  }

  .setting-control {
    display: block;
    width: min(100%, 15rem);
    min-width: 13rem;
    flex: 0 0 auto;
  }

  @media (max-width: 720px) {
    .setting-control {
      width: 100%;
      min-width: 0;
    }
  }
</style>
