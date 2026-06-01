<script lang="ts">
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
  } from '@shared/ui';
  import type { ThemeMode } from '@shared/theme';
  import type { LanguageMode } from '@shared/i18n';
  import { t } from '@shared/i18n';

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
    themeOptions.find((option) => option.value === themeMode)?.label ??
      t('settings.appearance.theme.placeholder'),
  );

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
          <Select
            type="single"
            items={themeOptions as SelectOption[]}
            value={themeMode}
            onValueChange={handleThemeChange}
          >
            <SelectTrigger class="w-60" aria-label={t('settings.appearance.theme.triggerLabel')}>
              {themeTriggerLabel}
            </SelectTrigger>
            <SelectContent>
              {#each themeOptions as option (option.value)}
                <SelectItem value={option.value} label={option.label} disabled={option.disabled}>
                  {option.label}
                </SelectItem>
              {/each}
            </SelectContent>
          </Select>
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
            <SelectTrigger class="w-60" aria-label={t('settings.appearance.language.triggerLabel')}>
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
