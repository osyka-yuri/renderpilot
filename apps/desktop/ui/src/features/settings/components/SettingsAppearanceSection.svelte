<script lang="ts">
  import Select from '@shared/ui/Select.svelte';
  import SettingsSectionShell from '@features/settings/components/SettingsSectionShell.svelte';
  import type { ThemeMode } from '@shared/theme/theme-mode';
  import type { LanguageMode, SelectOption } from '@features/settings/settings-screen-model';

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
    <div class="setting-row">
      <div class="setting-copy">
        <p class="setting-label">Display</p>
        <h4>Theme</h4>
        <p>
          Follow the operating system appearance or choose a fixed theme while keeping the
          application palette internally consistent.
        </p>
      </div>

      <span class="setting-control">
        <Select
          aria-label="Theme mode"
          options={themeOptions}
          value={themeMode}
          onValueChange={handleThemeChange}
        />
      </span>
    </div>

    <div class="setting-row">
      <div class="setting-copy">
        <p class="setting-label">Localization</p>
        <h4>Language</h4>
        <p>
          Use a scalable selector so more interface languages can be added later without changing
          the page structure.
        </p>
      </div>

      <span class="setting-control">
        <Select
          aria-label="Interface language"
          options={languageOptions}
          value={languageMode}
          onValueChange={handleLanguageChange}
        />
      </span>
    </div>
  </div>
</SettingsSectionShell>

<style>
  .settings-list {
    display: grid;
    gap: 1rem;
  }

  .setting-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem 1.5rem;
    padding-block: 1rem;
  }

  .setting-row:first-child {
    padding-top: 0;
  }

  .setting-row:last-child {
    padding-bottom: 0;
  }

  .setting-copy {
    min-width: 0;
    max-width: 42rem;
  }

  .setting-label {
    margin: 0 0 0.25rem;
    color: var(--text-muted, #667085);
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    line-height: 1.2;
    text-transform: uppercase;
  }

  .setting-copy h4 {
    margin: 0;
    color: var(--text-primary, #101828);
    font-size: 1rem;
    font-weight: 650;
    line-height: 1.4;
  }

  .setting-copy p:not(.setting-label) {
    margin: 0.35rem 0 0;
    color: var(--text-secondary, #475467);
    font-size: 0.925rem;
    line-height: 1.5;
  }

  .setting-control {
    display: block;
    width: min(100%, 15rem);
    min-width: 13rem;
    flex: 0 0 auto;
  }

  @media (max-width: 720px) {
    .setting-row {
      display: grid;
      gap: 0.75rem;
    }

    .setting-control {
      width: 100%;
      min-width: 0;
    }
  }
</style>
