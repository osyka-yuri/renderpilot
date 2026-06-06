<script lang="ts">
  import { onMount } from 'svelte';
  import type { ThemeMode } from '@shared/theme';
  import type { LanguageMode } from '@shared/i18n';
  import { t } from '@shared/i18n';
  import { ScrollArea, Tabs, TabsContent, TabsList, TabsTrigger } from '@shared/ui';
  import {
    type LanguageModeHandler,
    type ThemeModeHandler,
    languageOptions,
    themeOptions,
    tabOptions,
  } from '../model/settings-page-model';
  import {
    SettingsAppearanceSection,
    SettingsCatalogSection,
    SettingsAboutSection,
    createSettingsPanelModel,
  } from '@widgets/settings-panel';
  import { createAppUpdaterModel } from '@features/app-updater';

  type Props = {
    themeMode?: ThemeMode;
    languageMode?: LanguageMode;
    onThemeModeChange?: ThemeModeHandler;
    onLanguageModeChange?: LanguageModeHandler;
  };

  const {
    themeMode = 'system',
    languageMode = 'system',
    onThemeModeChange = () => undefined,
    onLanguageModeChange = () => undefined,
  }: Props = $props();

  const model = createSettingsPanelModel();
  const appUpdaterModel = createAppUpdaterModel();

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
  <TabsList class="grid w-full max-w-md shrink-0 grid-cols-2">
    {#each tabOptions as tab (tab.value)}
      <TabsTrigger value={tab.value}>{t(tab.labelKey)}</TabsTrigger>
    {/each}
  </TabsList>

  <TabsContent value="general" class="min-h-0 flex-1 overflow-hidden">
    <ScrollArea class="h-full">
      <div class="flex flex-col gap-6">
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
      </div>
    </ScrollArea>
  </TabsContent>

  <TabsContent value="catalog" class="min-h-0 flex-1 overflow-hidden">
    <ScrollArea class="h-full">
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
    </ScrollArea>
  </TabsContent>
</Tabs>
