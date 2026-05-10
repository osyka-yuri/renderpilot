<script lang="ts">
  import { onMount } from 'svelte';
  import type { ThemeMode } from '@shared/theme/theme-mode';
  import type { LanguageModeHandler, ThemeModeHandler, VoidHandler } from '@shared/utils/callbacks';
  import { getCatalogSetting, setCatalogSetting } from '@shared/api/desktop';
  import SettingsAppearanceSection from '@features/settings/components/SettingsAppearanceSection.svelte';
  import SettingsCatalogArtworkSection from '@features/settings/components/SettingsCatalogArtworkSection.svelte';
  import SettingsWorkflowSection from '@features/settings/components/SettingsWorkflowSection.svelte';
  import {
    loadCoverRemoteSources,
    loadSteamGridDbKey,
    persistCoverSourceToggle,
    saveSteamGridDbKey,
    type ArtworkControllerContext,
    type SteamKeyControllerContext,
  } from '@features/settings/settings-screen-controller';
  import { createDisposableRequestChannel } from '@shared/utils/request-channel';
  import {
    coverSourceToggleRows,
    isOptionValue,
    languageOptions,
    themeOptions,
    type CoverSourceToggleRow,
    type LanguageMode,
  } from '@features/settings/settings-screen-model';
  import {
    createInitialSettingsArtworkState,
    isCoverSourceDisabled as isCoverSourceDisabledState,
    type SettingsArtworkState,
  } from '@features/settings/settings-screen-state';

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

  let disposed = $state(true);

  let steamGridDbKeyInput = $state('');
  let steamKeyLoaded = $state(false);
  let steamKeyBusy = $state(false);
  let steamKeyMessage = $state('');

  let artworkState = $state<SettingsArtworkState>(createInitialSettingsArtworkState());
  let coverSourcesMessage = $state('');

  const steamKeyRequest = createDisposableRequestChannel(() => disposed);
  const artworkRequest = createDisposableRequestChannel(() => disposed);

  const steamKeyContext: SteamKeyControllerContext = {
    request: steamKeyRequest,
    getCatalogSetting,
    setCatalogSetting,
    state: {
      readInput: () => steamGridDbKeyInput,
      writeInput: (value) => {
        steamGridDbKeyInput = value;
      },
      setBusy: (value) => {
        steamKeyBusy = value;
      },
      setLoaded: (value) => {
        steamKeyLoaded = value;
      },
      setMessage: (value) => {
        steamKeyMessage = value;
      },
    },
  };

  const artworkContext: ArtworkControllerContext = {
    request: artworkRequest,
    getCatalogSetting,
    setCatalogSetting,
    state: {
      readState: () => artworkState,
      writeState: (nextState) => {
        artworkState = nextState;
      },
      setMessage: (value) => {
        coverSourcesMessage = value;
      },
    },
  };

  onMount(() => {
    disposed = false;
    loadInitialSettings();

    return () => {
      disposed = true;
    };
  });

  function loadInitialSettings(): void {
    void Promise.all([loadSteamGridDbKey(steamKeyContext), loadCoverRemoteSources(artworkContext)]);
  }

  function canSaveSteamGridDbKey(): boolean {
    return steamKeyLoaded && !steamKeyBusy;
  }

  function canReloadSteamGridDbKey(): boolean {
    return !steamKeyBusy;
  }

  function handleSteamGridDbKeySave(): void {
    if (!canSaveSteamGridDbKey()) {
      return;
    }

    void saveSteamGridDbKey(steamKeyContext);
  }

  function handleSteamGridDbKeyReload(): void {
    if (!canReloadSteamGridDbKey()) {
      return;
    }

    void loadSteamGridDbKey(steamKeyContext);
  }

  function handleCoverSourceToggle(row: CoverSourceToggleRow): void {
    if (isCoverSourceDisabled(row)) {
      return;
    }

    const previousEnabled = artworkState.coverSourcesState[row.policyKey];

    void persistCoverSourceToggle(artworkContext, row, !previousEnabled, previousEnabled);
  }

  function isCoverSourceDisabled(row: CoverSourceToggleRow): boolean {
    return isCoverSourceDisabledState(artworkState, row);
  }

  function handleThemeChange(nextValue: string): void {
    if (!isOptionValue(nextValue, themeOptions)) {
      return;
    }

    onThemeModeChange(nextValue);
  }

  function handleLanguageChange(nextValue: string): void {
    if (!isOptionValue(nextValue, languageOptions)) {
      return;
    }

    onLanguageModeChange(nextValue);
  }
</script>

<section class="screen-shell" aria-label="Settings">
  <SettingsAppearanceSection
    {themeMode}
    {languageMode}
    {themeOptions}
    {languageOptions}
    onThemeChange={handleThemeChange}
    onLanguageChange={handleLanguageChange}
  />

  <SettingsWorkflowSection {advancedMode} {onToggleAdvancedMode} />

  <SettingsCatalogArtworkSection
    {coverSourceToggleRows}
    coverSourcesState={artworkState.coverSourcesState}
    {isCoverSourceDisabled}
    onCoverSourceToggle={handleCoverSourceToggle}
    {coverSourcesMessage}
    bind:steamGridDbKeyInput
    {steamKeyLoaded}
    {steamKeyBusy}
    {steamKeyMessage}
    onSteamGridDbKeySave={handleSteamGridDbKeySave}
    onSteamGridDbKeyReload={handleSteamGridDbKeyReload}
  />
</section>

<style>
  .screen-shell {
    display: grid;
    gap: var(--space-5);
    width: 100%;
  }
</style>
