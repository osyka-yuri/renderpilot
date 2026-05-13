<script lang="ts">
  import { Select, SelectContent, SelectItem, SelectTrigger } from '@shared/ui';
  import SettingsSectionShell from './SettingsSectionShell.svelte';
  import SettingRow from './SettingRow.svelte';
  import type { ThemeMode } from '@shared/theme';
  import type { LanguageMode } from '@entities/settings';
  import { cn } from '@shared/classnames';

  type SelectOption<TValue extends string = string> = {
    value: TValue;
    label: string;
    disabled?: boolean;
  };

  type SelectChangeHandler<TValue extends string> = (value: TValue) => void;

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
    onThemeChange = () => undefined,
    onLanguageChange = () => undefined,
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

  const themeTriggerLabel = $derived(
    themeOptions.find((option) => option.value === themeMode)?.label ?? 'Select theme',
  );

  const languageTriggerLabel = $derived(
    languageOptions.find((option) => option.value === languageMode)?.label ?? 'Select language',
  );
</script>

<SettingsSectionShell
  titleId="appearance-title"
  eyebrow="Interface"
  title="Appearance and language"
  description="Keep the shell visually consistent across themes and languages without turning preferences into oversized cards."
>
  <div>
    <SettingRow>
      <div class="grid min-w-0 flex-1 gap-1">
        <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">Display</p>
        <h4>Theme</h4>
        <p>
          Follow the operating system appearance or choose a fixed theme while keeping the
          application palette internally consistent.
        </p>
      </div>

      <span class={cn('block w-full max-w-60 min-w-52 shrink-0', 'max-md:w-full max-md:min-w-0')}>
        <Select
          type="single"
          items={themeOptions as SelectOption[]}
          value={themeMode}
          onValueChange={handleThemeChange}
        >
          <SelectTrigger class="w-full" aria-label="Theme mode">{themeTriggerLabel}</SelectTrigger>
          <SelectContent>
            {#each themeOptions as option (option.value)}
              <SelectItem value={option.value} label={option.label} disabled={option.disabled}>
                {option.label}
              </SelectItem>
            {/each}
          </SelectContent>
        </Select>
      </span>
    </SettingRow>

    <SettingRow>
      <div class="grid min-w-0 flex-1 gap-1">
        <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
          Localization
        </p>
        <h4>Language</h4>
        <p>
          Use a scalable selector so more interface languages can be added later without changing
          the page structure.
        </p>
      </div>

      <span class={cn('block w-full max-w-60 min-w-52 shrink-0', 'max-md:w-full max-md:min-w-0')}>
        <Select
          type="single"
          items={languageOptions as SelectOption[]}
          value={languageMode}
          onValueChange={handleLanguageChange}
        >
          <SelectTrigger class="w-full" aria-label="Interface language">
            {languageTriggerLabel}
          </SelectTrigger>
          <SelectContent>
            {#each languageOptions as option (option.value)}
              <SelectItem value={option.value} label={option.label} disabled={option.disabled}>
                {option.label}
              </SelectItem>
            {/each}
          </SelectContent>
        </Select>
      </span>
    </SettingRow>
  </div>
</SettingsSectionShell>
