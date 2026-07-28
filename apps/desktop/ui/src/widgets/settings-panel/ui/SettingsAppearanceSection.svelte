<script lang="ts">
  import type { Component } from 'svelte';
  import MonitorIcon from '@lucide/svelte/icons/monitor';
  import SunIcon from '@lucide/svelte/icons/sun';
  import MoonIcon from '@lucide/svelte/icons/moon';
  import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemGroup,
    ItemSeparator,
    ItemTitle,
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    Spinner,
    ToggleGroup,
    ToggleGroupItem,
  } from '@shared/ui';
  import type { ThemeMode } from '@shared/theme';
  import type { LanguageMode } from '@shared/i18n';
  import { t } from '@shared/i18n';

  type SelectOption<TValue extends string = string> = {
    value: TValue;
    label: string;
    disabled?: boolean;
  };

  type ThemeChangeHandler = (value: ThemeMode) => void;
  type LanguageChangeHandler = (value: LanguageMode) => Promise<void>;

  type Props = {
    themeMode?: ThemeMode;
    languageMode?: LanguageMode;
    themeOptions?: readonly SelectOption<ThemeMode>[];
    languageOptions?: readonly SelectOption<LanguageMode>[];
    languageBusy?: boolean;
    onThemeChange?: ThemeChangeHandler;
    onLanguageChange?: LanguageChangeHandler;
  };

  const {
    themeMode = 'system',
    languageMode = 'system',
    themeOptions = [],
    languageOptions = [],
    languageBusy = false,
    onThemeChange = () => undefined,
    onLanguageChange = () => Promise.resolve(),
  }: Props = $props();

  const themeIcons: Record<ThemeMode, Component> = {
    system: MonitorIcon,
    light: SunIcon,
    dark: MoonIcon,
  };

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
    void onLanguageChange(value);
  }

  const languageTriggerLabel = $derived(
    languageOptions.find((option) => option.value === languageMode)?.label ??
      t('settings.appearance.language.placeholder'),
  );
</script>

<Card>
  <CardHeader>
    <CardTitle>{t('settings.appearance.title')}</CardTitle>
    <CardDescription>{t('settings.appearance.description')}</CardDescription>
  </CardHeader>
  <CardContent>
    <ItemGroup>
      <Item>
        <ItemContent>
          <ItemTitle>{t('settings.appearance.theme.title')}</ItemTitle>
          <ItemDescription>
            {t('settings.appearance.theme.description')}
          </ItemDescription>
        </ItemContent>
        <ItemActions>
          <ToggleGroup
            type="single"
            variant="outline"
            value={themeMode}
            onValueChange={handleThemeChange}
            aria-label={t('settings.appearance.theme.triggerLabel')}
          >
            {#each themeOptions as option (option.value)}
              {@const Icon = themeIcons[option.value]}
              <ToggleGroupItem
                value={option.value}
                disabled={option.disabled}
                aria-label={option.label}
              >
                <Icon aria-hidden="true" />
                {option.label}
              </ToggleGroupItem>
            {/each}
          </ToggleGroup>
        </ItemActions>
      </Item>

      <ItemSeparator />

      <Item>
        <ItemContent>
          <ItemTitle>{t('settings.appearance.language.title')}</ItemTitle>
          <ItemDescription>
            {t('settings.appearance.language.description')}
          </ItemDescription>
        </ItemContent>
        <ItemActions>
          <Select
            type="single"
            items={languageOptions as SelectOption[]}
            value={languageMode}
            onValueChange={handleLanguageChange}
          >
            <SelectTrigger
              class="w-60"
              aria-label={t('settings.appearance.language.triggerLabel')}
              aria-busy={languageBusy}
            >
              {#if languageBusy}
                <Spinner class="size-4" aria-hidden="true" />
              {/if}
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
        </ItemActions>
      </Item>
    </ItemGroup>
  </CardContent>
</Card>
