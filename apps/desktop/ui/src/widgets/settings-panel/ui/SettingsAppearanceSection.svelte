<script lang="ts">
  import { Select, type SelectOption } from '@shared/ui';
  import SettingsSectionShell from './SettingsSectionShell.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingCopy from './SettingCopy.svelte';
  import SettingLabel from './SettingLabel.svelte';
  import type { ThemeMode } from '@shared/theme';
  import type { LanguageMode } from '@entities/settings';
  import { cn } from '@shared/utils';

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

  const {
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
  <div>
    <SettingRow>
      <SettingCopy>
        <SettingLabel>Display</SettingLabel>
        <h4>Theme</h4>
        <p>
          Follow the operating system appearance or choose a fixed theme while keeping the
          application palette internally consistent.
        </p>
      </SettingCopy>

      <span class={cn('block w-full max-w-60 min-w-52 shrink-0', 'max-md:w-full max-md:min-w-0')}>
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

      <span class={cn('block w-full max-w-60 min-w-52 shrink-0', 'max-md:w-full max-md:min-w-0')}>
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
