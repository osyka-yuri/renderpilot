<script lang="ts">
  import { onMount } from 'svelte';
  import type { ThemeMode } from '@shared/theme';
  import type { VoidHandler } from '@shared/utils';
  import type { LanguageMode } from '@entities/settings';
  import {
    type LanguageModeHandler,
    type ThemeModeHandler,
    languageOptions,
    themeOptions,
  } from '../model/settings-page-model';
  import {
    SettingsAppearanceSection,
    SettingsCatalogArtworkSection,
    SettingsWorkflowSection,
    createSettingsPanelModel,
  } from '@widgets/settings-panel';

  const noopThemeChange: ThemeModeHandler = () => undefined;
  const noopLanguageChange: LanguageModeHandler = () => undefined;
  const noopToggle: VoidHandler = () => undefined;

  type Props = {
    themeMode?: ThemeMode;
    languageMode?: LanguageMode;
    advancedMode?: boolean;
    onThemeModeChange?: ThemeModeHandler;
    onLanguageModeChange?: LanguageModeHandler;
    onToggleAdvancedMode?: VoidHandler;
  };

  let {
    themeMode = 'system',
    languageMode = 'system',
    advancedMode = false,
    onThemeModeChange = noopThemeChange,
    onLanguageModeChange = noopLanguageChange,
    onToggleAdvancedMode = noopToggle,
  }: Props = $props();

  const model = createSettingsPanelModel();

  onMount(() => {
    model.init();

    return () => {
      model.dispose();
    };
  });
</script>

<section class="screen-shell" aria-label="Settings">
  <SettingsAppearanceSection
    {themeMode}
    {languageMode}
    {themeOptions}
    {languageOptions}
    onThemeChange={onThemeModeChange}
    onLanguageChange={onLanguageModeChange}
  />

  <SettingsWorkflowSection {advancedMode} {onToggleAdvancedMode} />

  <SettingsCatalogArtworkSection
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
    onSteamGridDbKeyReload={model.handleSteamGridDbKeyReload}
  />
</section>

<style>
  .screen-shell {
    display: grid;
    gap: var(--space-5);
    width: 100%;
  }
</style>
