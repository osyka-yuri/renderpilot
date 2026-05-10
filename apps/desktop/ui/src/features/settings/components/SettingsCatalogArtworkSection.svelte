<script lang="ts">
  import Button from '@shared/ui/Button.svelte';
  import Input from '@shared/ui/Input.svelte';
  import Switch from '@shared/ui/Switch.svelte';
  import SettingsSectionShell from '@features/settings/components/SettingsSectionShell.svelte';
  import type { CoverRemotePolicy } from '@shared/covers/cover-sync';
  import type { CoverSourceToggleRow } from '@features/settings/settings-screen-model';

  const STEAM_GRID_DB_KEY_INPUT_ID = 'steamgriddb-api-key';
  const STEAM_GRID_DB_KEY_MESSAGE_ID = 'steamgriddb-api-key-message';

  type Props = {
    coverSourceToggleRows?: readonly CoverSourceToggleRow[];
    coverSourcesState?: CoverRemotePolicy;
    isCoverSourceDisabled?: (row: CoverSourceToggleRow) => boolean;
    onCoverSourceToggle?: (row: CoverSourceToggleRow) => void;
    coverSourcesMessage?: string;
    steamGridDbKeyInput?: string;
    steamKeyLoaded?: boolean;
    steamKeyBusy?: boolean;
    steamKeyMessage?: string;
    onSteamGridDbKeySave?: () => void;
    onSteamGridDbKeyReload?: () => void;
  };

  let {
    coverSourceToggleRows = [],
    coverSourcesState = {
      steamCdn: true,
      gogCdn: true,
      steamgriddb: true,
    },
    isCoverSourceDisabled = () => false,
    onCoverSourceToggle = () => undefined,
    coverSourcesMessage = '',
    steamGridDbKeyInput = $bindable(''),
    steamKeyLoaded = false,
    steamKeyBusy = false,
    steamKeyMessage = '',
    onSteamGridDbKeySave = () => undefined,
    onSteamGridDbKeyReload = () => undefined,
  }: Props = $props();

  const isSteamKeyEditable = $derived(steamKeyLoaded && !steamKeyBusy);
  const isSteamKeyReloadDisabled = $derived(steamKeyBusy);
  const steamKeyPlaceholder = $derived(steamKeyLoaded ? 'Bearer token' : 'Loading…');
  const steamKeyMessageId = $derived(steamKeyMessage ? STEAM_GRID_DB_KEY_MESSAGE_ID : undefined);

  const isCoverSourceChecked = (row: CoverSourceToggleRow): boolean => {
    return coverSourcesState[row.policyKey];
  };

  const handleCoverSourceToggle = (row: CoverSourceToggleRow): void => {
    if (isCoverSourceDisabled(row)) {
      return;
    }

    onCoverSourceToggle(row);
  };

  const handleSteamGridDbKeySave = (): void => {
    if (!isSteamKeyEditable) {
      return;
    }

    onSteamGridDbKeySave();
  };

  const handleSteamGridDbKeyReload = (): void => {
    if (isSteamKeyReloadDisabled) {
      return;
    }

    onSteamGridDbKeyReload();
  };
</script>

<SettingsSectionShell
  titleId="catalog-art-title"
  eyebrow="Catalog"
  title="Game artwork"
  description="Choose which remote sources may run when downloading artwork automatically. SteamGridDB still needs an API key below; disabling it skips remote search entirely."
>
  {#each coverSourceToggleRows as row (row.settingKey)}
    <div class="setting-row switch-row">
      <Switch
        checked={isCoverSourceChecked(row)}
        disabled={isCoverSourceDisabled(row)}
        aria-label={row.ariaLabel}
        onCheckedChange={() => {
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
      <p class="catalog-setting-hint" role="status" aria-live="polite">
        {coverSourcesMessage}
      </p>
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

    <div class="catalog-setting-stack" aria-busy={steamKeyBusy}>
      <label class="sr-only" for={STEAM_GRID_DB_KEY_INPUT_ID}>SteamGridDB API key</label>

      <Input
        id={STEAM_GRID_DB_KEY_INPUT_ID}
        type="password"
        autocomplete="off"
        placeholder={steamKeyPlaceholder}
        bind:value={steamGridDbKeyInput}
        disabled={!isSteamKeyEditable}
        aria-describedby={steamKeyMessageId}
      />

      <div class="catalog-setting-actions">
        <Button
          variant="primary"
          size="sm"
          disabled={!isSteamKeyEditable}
          onclick={handleSteamGridDbKeySave}
        >
          Save key
        </Button>

        <Button
          variant="secondary"
          size="sm"
          disabled={isSteamKeyReloadDisabled}
          onclick={handleSteamGridDbKeyReload}
        >
          Reload
        </Button>
      </div>

      {#if steamKeyMessage}
        <p
          id={STEAM_GRID_DB_KEY_MESSAGE_ID}
          class="catalog-setting-hint"
          role="status"
          aria-live="polite"
        >
          {steamKeyMessage}
        </p>
      {/if}
    </div>
  </div>
</SettingsSectionShell>

<style>
  .switch-row {
    padding-block: var(--space-4);
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

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    clip-path: inset(50%);
    white-space: nowrap;
    border: 0;
  }

  @media (max-width: 720px) {
    .catalog-setting-stack {
      width: 100%;
    }
  }
</style>
