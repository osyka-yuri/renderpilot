<script lang="ts">
  import { onMount } from 'svelte';
  import type { Component } from 'svelte';
  import SlidersHorizontalIcon from '@lucide/svelte/icons/sliders-horizontal';
  import ImageIcon from '@lucide/svelte/icons/image';
  import CpuIcon from '@lucide/svelte/icons/cpu';
  import WandIcon from '@lucide/svelte/icons/wand';
  import type { ThemeMode } from '@shared/theme';
  import type { LanguageMode } from '@shared/i18n';
  import { t } from '@shared/i18n';
  import { Tabs, TabsList, TabsTrigger } from '@shared/ui';
  import type { SettingsUpdateAction } from '@features/app-updater';
  import {
    createDlssIndicatorContext,
    createGlobalNvidiaPresetsContext,
  } from '@features/nvapi-settings';
  import {
    type LanguageModeHandler,
    type ThemeModeHandler,
    type SettingsTabValue,
    languageOptions,
    themeOptions,
    tabOptions,
    settingsTabMemory,
  } from '../model/settings-page-model';
  import SettingsTabPanel from './SettingsTabPanel.svelte';
  import {
    SettingsAppearanceSection,
    SettingsCatalogSection,
    SettingsNvidiaSection,
    SettingsRenoDxSection,
    SettingsAboutSection,
    createSettingsPanelModel,
  } from '@widgets/settings-panel';

  type Props = {
    isElevated?: boolean;
    themeMode?: ThemeMode;
    languageMode?: LanguageMode;
    languageBusy?: boolean;
    appVersion?: string | null;
    updateAction?: SettingsUpdateAction;
    onThemeModeChange?: ThemeModeHandler;
    onLanguageModeChange?: LanguageModeHandler;
    onCheckForUpdates?: () => void;
  };

  const {
    isElevated = false,
    themeMode = 'system',
    languageMode = 'system',
    languageBusy = false,
    appVersion = null,
    updateAction = 'check',
    onThemeModeChange = () => undefined,
    onLanguageModeChange = () => Promise.resolve(),
    onCheckForUpdates = () => undefined,
  }: Props = $props();

  const model = createSettingsPanelModel();
  const dlssIndicator = createDlssIndicatorContext({ isElevated: () => isElevated });
  const globalPresets = createGlobalNvidiaPresetsContext({ isElevated: () => isElevated });

  const localizedThemeOptions = $derived(
    themeOptions.map((option) => ({ value: option.value, label: t(option.labelKey) })),
  );
  const localizedLanguageOptions = $derived(
    languageOptions.map((option) => ({ value: option.value, label: t(option.labelKey) })),
  );

  const tabIcons: Record<SettingsTabValue, Component> = {
    general: SlidersHorizontalIcon,
    renodx: WandIcon,
    catalog: ImageIcon,
    nvidia: CpuIcon,
  };

  let activeTab = $state<SettingsTabValue>(settingsTabMemory.getInitialTab());
  $effect(() => {
    settingsTabMemory.rememberTab(activeTab);
  });

  onMount(() => {
    model.init();

    return () => {
      model.dispose();
    };
  });
</script>

<Tabs bind:value={activeTab} class="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden">
  <TabsList class="shrink-0">
    {#each tabOptions as tab (tab.value)}
      {@const Icon = tabIcons[tab.value]}
      <TabsTrigger value={tab.value}>
        <Icon aria-hidden="true" />
        {t(tab.labelKey)}
      </TabsTrigger>
    {/each}
  </TabsList>

  <SettingsTabPanel value="general">
    <SettingsAppearanceSection
      {themeMode}
      {languageMode}
      {languageBusy}
      themeOptions={localizedThemeOptions}
      languageOptions={localizedLanguageOptions}
      onThemeChange={onThemeModeChange}
      onLanguageChange={onLanguageModeChange}
    />
    <SettingsAboutSection {appVersion} {updateAction} {onCheckForUpdates} />
  </SettingsTabPanel>

  <SettingsTabPanel value="renodx">
    <SettingsRenoDxSection />
  </SettingsTabPanel>

  <SettingsTabPanel value="catalog">
    <SettingsCatalogSection
      coverSourceToggleRows={model.coverSourceToggleRows}
      coverSourcesState={model.coverSourcesState}
      isCoverSourceDisabled={model.isCoverSourceDisabled}
      onCoverSourceToggle={model.handleCoverSourceToggle}
      coverSourcesMessage={model.coverSourcesMessage}
      coverSourcesMessageKind={model.coverSourcesMessageKind}
      bind:steamGridDbKeyInput={model.steamGridDbKeyInput}
      steamKeyLoaded={model.steamKeyLoaded}
      steamKeyBusy={model.steamKeyBusy}
      steamKeyMessage={model.steamKeyMessage}
      steamKeyMessageKind={model.steamKeyMessageKind}
      onSteamGridDbKeySave={model.handleSteamGridDbKeySave}
    />
  </SettingsTabPanel>

  <SettingsTabPanel value="nvidia">
    <SettingsNvidiaSection {dlssIndicator} {globalPresets} />
  </SettingsTabPanel>
</Tabs>
