<script lang="ts">
  import { onMount, untrack } from 'svelte';
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
    createNvapiViewRefresh,
  } from '@features/nvapi-settings';
  import { createExclusiveTaskRunner, type ExclusiveTaskRunner } from '@shared/concurrency';
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
    themeMode?: ThemeMode;
    languageMode?: LanguageMode;
    languageBusy?: boolean;
    appVersion?: string | null;
    updateAction?: SettingsUpdateAction;
    nvapiMutationRunner?: ExclusiveTaskRunner;
    onThemeModeChange?: ThemeModeHandler;
    onLanguageModeChange?: LanguageModeHandler;
    onCheckForUpdates?: () => void;
  };

  const {
    themeMode = 'system',
    languageMode = 'system',
    languageBusy = false,
    appVersion = null,
    updateAction = 'check',
    nvapiMutationRunner: providedNvapiMutationRunner,
    onThemeModeChange = () => undefined,
    onLanguageModeChange = () => Promise.resolve(),
    onCheckForUpdates = () => undefined,
  }: Props = $props();
  const nvapiMutationRunner = untrack(
    () => providedNvapiMutationRunner ?? createExclusiveTaskRunner(),
  );

  const model = createSettingsPanelModel();
  const dlssIndicator = createDlssIndicatorContext();
  const globalPresets = createGlobalNvidiaPresetsContext(nvapiMutationRunner);
  const nvapiViewRefresh = createNvapiViewRefresh(() => void globalPresets.refresh());

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
  let hasVisitedNvidia = false;
  $effect(() => {
    settingsTabMemory.rememberTab(activeTab);
  });

  $effect(() => {
    const currentTab = activeTab;
    if (currentTab !== 'nvidia' && !hasVisitedNvidia) {
      return;
    }
    if (currentTab === 'nvidia') {
      hasVisitedNvidia = true;
    }
    const ready = globalPresets.loaded && !globalPresets.busy;
    untrack(() => {
      nvapiViewRefresh.sync(currentTab === 'nvidia' ? 'global-nvidia-settings' : null, ready);
    });
  });

  onMount(() => {
    model.init();

    return () => {
      nvapiViewRefresh.destroy();
      model.dispose();
    };
  });
</script>

<section class="flex min-h-0 flex-1 flex-col overflow-hidden" aria-labelledby="settings-title">
  <h1 id="settings-title" class="sr-only">{t('nav.settings')}</h1>
  <Tabs bind:value={activeTab} class="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden">
    <TabsList class="shrink-0" aria-label={t('nav.settings')}>
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
      {#if activeTab === 'nvidia'}
        <SettingsNvidiaSection {dlssIndicator} {globalPresets} />
      {/if}
    </SettingsTabPanel>
  </Tabs>
</section>
