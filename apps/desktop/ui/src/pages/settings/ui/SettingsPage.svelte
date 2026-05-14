<script lang="ts">
  import { onMount } from 'svelte';
  import type { ThemeMode } from '@shared/theme';
  import type { LanguageMode } from '@entities/settings';
  import { Tabs, TabsContent, TabsList, TabsTrigger } from '@shared/ui';
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
    createSettingsPanelModel,
  } from '@widgets/settings-panel';

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

  onMount(() => {
    model.init();

    return () => {
      model.dispose();
    };
  });
</script>

<Tabs value="appearance">
  <TabsList class="grid w-full max-w-md grid-cols-2">
    {#each tabOptions as tab (tab.value)}
      <TabsTrigger value={tab.value}>{tab.label}</TabsTrigger>
    {/each}
  </TabsList>

  <TabsContent value="appearance">
    <SettingsAppearanceSection
      {themeMode}
      {languageMode}
      {themeOptions}
      {languageOptions}
      onThemeChange={onThemeModeChange}
      onLanguageChange={onLanguageModeChange}
    />
  </TabsContent>

  <TabsContent value="catalog">
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
  </TabsContent>
</Tabs>
