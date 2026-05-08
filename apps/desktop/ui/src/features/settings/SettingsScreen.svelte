<script lang="ts">
  import { onMount } from 'svelte';
  import type { LanguageModeHandler, ThemeModeHandler, VoidHandler } from '@shared/utils/callbacks';
  import type { ThemeMode } from '@shared/theme/theme-mode';

  import { getCatalogSetting, setCatalogSetting } from '@shared/api/desktop';
  import {
    COVERS_GOG_CDN_SETTING_KEY,
    COVERS_STEAM_CDN_SETTING_KEY,
    COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
    STEAMGRIDDB_SETTING_KEY,
  } from '@shared/catalog/catalog-setting-keys';
  import { fetchCoverRemotePolicy, type CoverRemotePolicy } from '@shared/covers/cover-sync';

  import Badge from '@shared/ui/Badge.svelte';
  import Button from '@shared/ui/Button.svelte';
  import Select from '@shared/ui/Select.svelte';
  import Surface from '@shared/ui/Surface.svelte';
  import Switch from '@shared/ui/Switch.svelte';

  type SelectOption<Value extends string> = {
    value: Value;
    label: string;
  };

  type LanguageMode = 'system' | 'en' | 'ru';
  type CoverSourcePolicyKey = keyof CoverRemotePolicy;

  type CoverSourceSettingKey =
    | typeof COVERS_STEAM_CDN_SETTING_KEY
    | typeof COVERS_GOG_CDN_SETTING_KEY
    | typeof COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY;

  type CoverSourceToggleRow = {
    settingKey: CoverSourceSettingKey;
    policyKey: CoverSourcePolicyKey;
    ariaLabel: string;
    eyebrow: string;
    title: string;
    description: string;
  };

  const themeOptions = [
    { value: 'system', label: 'System' },
    { value: 'dark', label: 'Dark' },
    { value: 'light', label: 'Light' },
  ] satisfies readonly SelectOption<ThemeMode>[];

  const languageOptions = [
    { value: 'system', label: 'Follow system' },
    { value: 'en', label: 'English' },
    { value: 'ru', label: 'Russian' },
  ] satisfies readonly SelectOption<LanguageMode>[];

  const coverSourceToggleRows = [
    {
      settingKey: COVERS_STEAM_CDN_SETTING_KEY,
      policyKey: 'steamCdn',
      ariaLabel: 'Use Steam CDN for artwork',
      eyebrow: 'Steam',
      title: 'Steam CDN',
      description: 'Public Steam library artwork when the catalog has a Steam app id.',
    },
    {
      settingKey: COVERS_GOG_CDN_SETTING_KEY,
      policyKey: 'gogCdn',
      ariaLabel: 'Use GOG CDN for artwork',
      eyebrow: 'GOG',
      title: 'GOG CDN',
      description: 'GOG vertical covers when the catalog has a numeric GOG product id.',
    },
    {
      settingKey: COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
      policyKey: 'steamgriddb',
      ariaLabel: 'Use SteamGridDB for artwork search',
      eyebrow: 'SteamGridDB',
      title: 'Remote search',
      description:
        'Slug lookups, autocomplete, and grid images via the SteamGridDB API (requires a key).',
    },
  ] satisfies readonly CoverSourceToggleRow[];

  const defaultCoverSourcesState: CoverRemotePolicy = {
    steamCdn: true,
    gogCdn: true,
    steamgriddb: true,
  };

  const catalogReadError = 'Could not read catalog settings.';
  const steamKeySaveError = 'Could not save API key.';
  const artworkSettingsReadError = 'Could not read automatic artwork settings.';
  const artworkSourceSaveError = 'Could not save artwork source setting.';

  const noopThemeChange: ThemeModeHandler = () => undefined;
  const noopLanguageChange: LanguageModeHandler = () => undefined;
  const noopToggle: VoidHandler = () => undefined;

  export let themeMode: ThemeMode = 'system';
  export let languageMode: LanguageMode = 'system';
  export let advancedMode = false;

  export let onThemeModeChange: ThemeModeHandler = noopThemeChange;
  export let onLanguageModeChange: LanguageModeHandler = noopLanguageChange;
  export let onToggleAdvancedMode: VoidHandler = noopToggle;

  let steamGridDbKeyInput = '';
  let steamKeyLoaded = false;
  let steamKeyBusy = false;
  let steamKeyMessage = '';

  let coverSourcesState: CoverRemotePolicy = { ...defaultCoverSourcesState };
  let coverSourcesLoaded = false;
  let coverSourcesBusy = false;
  let coverSourcesMessage = '';
  let savingCoverSourceKeys = new Set<CoverSourceSettingKey>();

  onMount(() => {
    void initializeSettings();
  });

  async function initializeSettings(): Promise<void> {
    await Promise.all([loadSteamGridDbKey(), loadCoverRemoteSources()]);
  }

  async function loadSteamGridDbKey(): Promise<void> {
    steamKeyBusy = true;
    steamKeyMessage = '';

    try {
      const payload = await getCatalogSetting(STEAMGRIDDB_SETTING_KEY);

      steamGridDbKeyInput = payload.value ?? '';
      steamKeyLoaded = true;
    } catch {
      steamKeyMessage = catalogReadError;
      steamKeyLoaded = true;
    } finally {
      steamKeyBusy = false;
    }
  }

  async function loadCoverRemoteSources(): Promise<void> {
    coverSourcesBusy = true;
    coverSourcesMessage = '';

    try {
      const policy = await fetchCoverRemotePolicy(getCatalogSetting);

      coverSourcesState = {
        steamCdn: policy.steamCdn,
        gogCdn: policy.gogCdn,
        steamgriddb: policy.steamgriddb,
      };
    } catch {
      coverSourcesMessage = artworkSettingsReadError;
    } finally {
      coverSourcesLoaded = true;
      coverSourcesBusy = false;
    }
  }

  async function saveSteamGridDbKey(): Promise<void> {
    steamKeyBusy = true;
    steamKeyMessage = '';

    const normalizedKey = steamGridDbKeyInput.trim();

    try {
      await setCatalogSetting(STEAMGRIDDB_SETTING_KEY, normalizedKey);

      steamGridDbKeyInput = normalizedKey;
      steamKeyMessage = normalizedKey ? 'Saved.' : 'Key cleared.';
    } catch {
      steamKeyMessage = steamKeySaveError;
    } finally {
      steamKeyBusy = false;
    }
  }

  function handleSteamGridDbKeySave(): void {
    if (steamKeyBusy || !steamKeyLoaded) {
      return;
    }

    void saveSteamGridDbKey();
  }

  function handleSteamGridDbKeyReload(): void {
    if (steamKeyBusy) {
      return;
    }

    void loadSteamGridDbKey();
  }

  function handleCoverSourceToggle(row: CoverSourceToggleRow): void {
    if (isCoverSourceDisabled(row)) {
      return;
    }

    const previousEnabled = coverSourcesState[row.policyKey];

    void persistCoverSourceToggle(row, !previousEnabled, previousEnabled);
  }

  async function persistCoverSourceToggle(
    row: CoverSourceToggleRow,
    nextEnabled: boolean,
    previousEnabled: boolean,
  ): Promise<void> {
    setCoverSourceSaving(row.settingKey, true);
    setCoverSourceValue(row.policyKey, nextEnabled);
    coverSourcesMessage = '';

    try {
      await setCatalogSetting(row.settingKey, formatBooleanSetting(nextEnabled));
    } catch {
      setCoverSourceValue(row.policyKey, previousEnabled);
      coverSourcesMessage = artworkSourceSaveError;
    } finally {
      setCoverSourceSaving(row.settingKey, false);
    }
  }

  function setCoverSourceValue(policyKey: CoverSourcePolicyKey, enabled: boolean): void {
    coverSourcesState = {
      ...coverSourcesState,
      [policyKey]: enabled,
    };
  }

  function setCoverSourceSaving(settingKey: CoverSourceSettingKey, saving: boolean): void {
    const nextSavingKeys = new Set(savingCoverSourceKeys);

    if (saving) {
      nextSavingKeys.add(settingKey);
    } else {
      nextSavingKeys.delete(settingKey);
    }

    savingCoverSourceKeys = nextSavingKeys;
  }

  function isCoverSourceSaving(settingKey: CoverSourceSettingKey): boolean {
    return savingCoverSourceKeys.has(settingKey);
  }

  function isCoverSourceDisabled(row: CoverSourceToggleRow): boolean {
    return !coverSourcesLoaded || coverSourcesBusy || isCoverSourceSaving(row.settingKey);
  }

  function formatBooleanSetting(value: boolean): string {
    return value ? 'true' : 'false';
  }

  function isOptionValue<Value extends string>(
    value: string,
    options: readonly SelectOption<Value>[],
  ): value is Value {
    return options.some((option) => option.value === value);
  }

  function handleThemeChange(nextValue: string): void {
    if (isOptionValue(nextValue, themeOptions)) {
      onThemeModeChange(nextValue);
    }
  }

  function handleLanguageChange(nextValue: string): void {
    if (isOptionValue(nextValue, languageOptions)) {
      onLanguageModeChange(nextValue);
    }
  }

  function handleAdvancedModeToggle(): void {
    onToggleAdvancedMode();
  }
</script>

<section class="screen-shell" aria-label="Settings">
  <article class="settings-section" aria-labelledby="appearance-title">
    <header class="section-header">
      <p class="eyebrow">Interface</p>
      <h3 id="appearance-title">Appearance and language</h3>
      <p class="section-copy">
        Keep the shell visually consistent across themes and languages without turning preferences
        into oversized cards.
      </p>
    </header>

    <Surface className="settings-panel" tone="elevated" shadow>
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
            ariaLabel="Theme mode"
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
            ariaLabel="Interface language"
            options={languageOptions}
            value={languageMode}
            onValueChange={handleLanguageChange}
          />
        </span>
      </div>
    </Surface>
  </article>

  <article class="settings-section" aria-labelledby="workflow-title">
    <header class="section-header">
      <p class="eyebrow">Behavior</p>
      <h3 id="workflow-title">Workflow and provider posture</h3>
      <p class="section-copy">
        Keep operational behavior predictable and expose lower-level controls only when they improve
        the workflow.
      </p>
    </header>

    <Surface className="settings-panel" tone="elevated" shadow>
      <div class="setting-row switch-row">
        <Switch
          checked={advancedMode}
          aria-label="Advanced mode"
          onclick={handleAdvancedModeToggle}
        >
          <span class="setting-copy">
            <span class="setting-label">Detail level</span>
            <span class="row-title">Advanced mode</span>
            <span class="row-copy">
              Show lower-level actions and technical controls in detail screens only when you need
              them.
            </span>
          </span>
        </Switch>
      </div>

      <div class="setting-row status-row">
        <div class="setting-copy">
          <p class="setting-label">Discovery</p>
          <h4>Scan source</h4>
          <p>
            Manual folder scanning is active. Provider integrations can be added later without
            changing the overall settings hierarchy.
          </p>
        </div>

        <div class="setting-status">
          <Badge pill size="md" tone="muted">Manual scan</Badge>
          <span class="status-note">Current source</span>
        </div>
      </div>
    </Surface>
  </article>

  <article class="settings-section" aria-labelledby="catalog-art-title">
    <header class="section-header">
      <p class="eyebrow">Catalog</p>
      <h3 id="catalog-art-title">Game artwork</h3>
      <p class="section-copy">
        Choose which remote sources may run when downloading artwork automatically. SteamGridDB
        still needs an API key below; disabling it skips remote search entirely.
      </p>
    </header>

    <Surface className="settings-panel" tone="elevated" shadow>
      {#each coverSourceToggleRows as row (row.settingKey)}
        <div class="setting-row switch-row">
          <Switch
            checked={coverSourcesState[row.policyKey]}
            disabled={isCoverSourceDisabled(row)}
            aria-label={row.ariaLabel}
            onclick={() => {
              handleCoverSourceToggle(row);
            }}
          >
            <span class="setting-copy">
              <span class="setting-label">{row.eyebrow}</span>
              <span class="row-title">{row.title}</span>
              <span class="row-copy">{row.description}</span>
            </span>
          </Switch>
        </div>
      {/each}

      {#if coverSourcesMessage}
        <div class="setting-row catalog-sources-hint-row">
          <p class="catalog-setting-hint" aria-live="polite">{coverSourcesMessage}</p>
        </div>
      {/if}

      <div class="setting-row catalog-setting-row">
        <div class="setting-copy">
          <p class="setting-label">SteamGridDB</p>
          <h4>API key</h4>
          <p>
            Create a key at steamgriddb.com and paste it here to enable artwork search for non-Steam
            titles and CDN fallbacks.
          </p>
        </div>

        <div class="catalog-setting-stack">
          <label class="sr-only" for="steamgriddb-api-key">SteamGridDB API key</label>
          <input
            id="steamgriddb-api-key"
            class="text-input"
            type="password"
            autocomplete="off"
            placeholder={steamKeyLoaded ? 'Bearer token' : 'Loading…'}
            bind:value={steamGridDbKeyInput}
            disabled={steamKeyBusy || !steamKeyLoaded}
          />

          <div class="catalog-setting-actions">
            <Button
              variant="primary"
              size="sm"
              disabled={steamKeyBusy || !steamKeyLoaded}
              onclick={handleSteamGridDbKeySave}
            >
              Save key
            </Button>

            <Button
              variant="secondary"
              size="sm"
              disabled={steamKeyBusy}
              onclick={handleSteamGridDbKeyReload}
            >
              Reload
            </Button>
          </div>

          {#if steamKeyMessage}
            <p class="catalog-setting-hint" aria-live="polite">{steamKeyMessage}</p>
          {/if}
        </div>
      </div>
    </Surface>
  </article>
</section>

<style>
  .screen-shell,
  .settings-section,
  .section-header,
  .setting-copy,
  .setting-status {
    display: grid;
  }

  .screen-shell {
    gap: var(--space-5);
    width: 100%;
  }

  .settings-section {
    gap: var(--space-3);
  }

  .section-header {
    gap: var(--space-1);
    padding-inline: var(--space-1);
  }

  .eyebrow,
  .setting-label {
    margin: 0;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  h3,
  h4 {
    margin: 0;
    font-weight: 600;
  }

  h3 {
    font-size: 1.05rem;
  }

  h4,
  .row-title {
    color: var(--text-strong);
    font-size: 0.95rem;
    font-weight: 600;
  }

  .section-copy {
    max-width: 56rem;
    margin: 0;
    font-size: 0.875rem;
    line-height: 1.45;
  }

  :global(.settings-panel) {
    display: grid;
    gap: 0;
    overflow: hidden;
    border-radius: var(--radius-xl);
  }

  :global(.settings-panel) > :last-child {
    border-bottom: 0;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4);
    border-bottom: 1px solid var(--border-subtle);
  }

  .setting-copy {
    flex: 1;
    min-width: 0;
    gap: var(--space-1);
  }

  .setting-copy p,
  .row-copy {
    margin: 0;
    font-size: 0.84rem;
    line-height: 1.45;
  }

  .setting-control {
    display: block;
    width: min(100%, 15rem);
    min-width: 13rem;
    flex-shrink: 0;
  }

  .switch-row {
    padding-block: var(--space-4);
  }

  .status-row {
    align-items: flex-start;
  }

  .setting-status {
    flex-shrink: 0;
    justify-items: end;
    gap: var(--space-2);
  }

  .status-note {
    color: var(--text-muted);
    font-size: 0.74rem;
  }

  .catalog-setting-row {
    align-items: flex-start;
  }

  .catalog-sources-hint-row {
    padding-block: var(--space-2);
  }

  .catalog-sources-hint-row .catalog-setting-hint {
    margin-inline: var(--space-4);
  }

  .catalog-setting-stack {
    display: grid;
    gap: var(--space-2);
    width: min(100%, 22rem);
    flex-shrink: 0;
  }

  .catalog-setting-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .catalog-setting-hint {
    margin: 0;
    font-size: 0.78rem;
    color: var(--text-muted);
  }

  .text-input {
    width: 100%;
    padding: 0.55rem 0.65rem;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-subtle);
    background: var(--bg-control);
    color: var(--text-strong);
    font-size: 0.875rem;
  }

  .text-input:disabled {
    opacity: 0.65;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  @media (max-width: 720px) {
    .section-header {
      padding-inline: 0;
    }

    .setting-row {
      flex-direction: column;
      align-items: stretch;
      gap: 0.75rem;
    }

    .setting-control {
      width: 100%;
      min-width: 0;
    }

    .setting-status {
      justify-items: start;
    }

    .catalog-setting-stack {
      width: 100%;
    }
  }
</style>
