<script lang="ts">
  import { onMount } from 'svelte';
  import type { ThemeMode } from '@shared/theme';
  import type { LanguageMode } from '@shared/i18n';
  import { t } from '@shared/i18n';
  import { Tabs, TabsList, TabsTrigger } from '@shared/ui';
  import { createDlssIndicatorContext } from '@features/nvapi-settings';
  import {
    type LanguageModeHandler,
    type ThemeModeHandler,
    languageOptions,
    themeOptions,
    tabOptions,
  } from '../model/settings-page-model';
  import SettingsTabPanel from './SettingsTabPanel.svelte';
  import {
    SettingsAppearanceSection,
    SettingsCatalogSection,
    SettingsNvidiaSection,
    SettingsAboutSection,
    createSettingsPanelModel,
  } from '@widgets/settings-panel';
  import { createAppUpdaterModel } from '@features/app-updater';

  type Props = {
    isElevated?: boolean;
    themeMode?: ThemeMode;
    languageMode?: LanguageMode;
    onThemeModeChange?: ThemeModeHandler;
    onLanguageModeChange?: LanguageModeHandler;
  };

  const {
    isElevated = false,
    themeMode = 'system',
    languageMode = 'system',
    onThemeModeChange = () => undefined,
    onLanguageModeChange = () => undefined,
  }: Props = $props();

  const model = createSettingsPanelModel();
  const appUpdaterModel = createAppUpdaterModel();
  const dlssIndicator = createDlssIndicatorContext({ isElevated: () => isElevated });

  const localizedThemeOptions = $derived(
    themeOptions.map((option) => ({ value: option.value, label: t(option.labelKey) })),
  );
  const localizedLanguageOptions = $derived(
    languageOptions.map((option) => ({ value: option.value, label: t(option.labelKey) })),
  );

  onMount(() => {
    model.init();
    void appUpdaterModel.init();

    return () => {
      model.dispose();
    };
  });
</script>

<Tabs value="general" class="flex h-full flex-col">
  <TabsList class="grid w-full max-w-md shrink-0 grid-cols-3">
    {#each tabOptions as tab (tab.value)}
      <TabsTrigger value={tab.value}>{t(tab.labelKey)}</TabsTrigger>
    {/each}
  </TabsList>

  <SettingsTabPanel value="general">
    <SettingsAppearanceSection
      {themeMode}
      {languageMode}
      themeOptions={localizedThemeOptions}
      languageOptions={localizedLanguageOptions}
      onThemeChange={onThemeModeChange}
      onLanguageChange={onLanguageModeChange}
    />
    <SettingsAboutSection
      appVersion={appUpdaterModel.appVersion}
      isCheckingForUpdates={appUpdaterModel.isCheckingForUpdates}
      isDownloading={appUpdaterModel.isDownloading}
      onCheckForUpdates={appUpdaterModel.handleCheckForUpdates}
    />
  </SettingsTabPanel>

  <SettingsTabPanel value="catalog">
    <SettingsCatalogSection
      coverSourceToggleRows={model.coverSourceToggleRows}
      coverSourcesState={model.coverSourcesState}
      isCoverSourceDisabled={model.isCoverSourceDisabled}
      onCoverSourceToggle={model.handleCoverSourceToggle}
      coverSourcesMessage={model.coverSourcesMessage}
      bind:steamGridDbKeyInput={model.steamGridDbKeyInput}
      steamKeyLoaded={model.steamKeyLoaded}
      steamKeyBusy={model.steamKeyBusy}
      steamKeyMessage={model.steamKeyMessage}
      onSteamGridDbKeySave={model.handleSteamGridDbKeySave}
    />
  </SettingsTabPanel>

  <SettingsTabPanel value="nvidia">
    <SettingsNvidiaSection {dlssIndicator} />
  </SettingsTabPanel>
</Tabs>
