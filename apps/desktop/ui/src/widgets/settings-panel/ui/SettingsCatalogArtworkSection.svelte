<script lang="ts">
  import { Button, Input, Switch } from '@shared/ui';
  import SettingsSectionShell from './SettingsSectionShell.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingCopy from './SettingCopy.svelte';
  import SettingLabel from './SettingLabel.svelte';
  import type { CoverRemotePolicy } from '@entities/settings';
  import type { CoverSourceToggleRow } from '@features/settings-artwork';

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
    <SettingRow>
      <Switch
        checked={isCoverSourceChecked(row)}
        disabled={isCoverSourceDisabled(row)}
        aria-label={row.ariaLabel}
        onCheckedChange={() => {
          handleCoverSourceToggle(row);
        }}
      >
        <SettingCopy>
          <SettingLabel>{row.eyebrow}</SettingLabel>
          <span class="row-title">{row.title}</span>
          <span class="row-copy">{row.description}</span>
        </SettingCopy>
      </Switch>
    </SettingRow>
  {/each}

  {#if coverSourcesMessage}
    <SettingRow>
      <div class="catalog-sources-hint">
        <p class="catalog-setting-hint" role="status" aria-live="polite">
          {coverSourcesMessage}
        </p>
      </div>
    </SettingRow>
  {/if}

  <SettingRow>
    <SettingCopy>
      <SettingLabel>SteamGridDB</SettingLabel>
      <h4>API key</h4>
      <p>
        Create a key at steamgriddb.com and paste it here to enable artwork search for non-Steam
        titles and CDN fallbacks.
      </p>
    </SettingCopy>

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
  </SettingRow>
</SettingsSectionShell>

<style>
  .catalog-sources-hint {
    padding-block: var(--space-2);
    padding-inline: var(--space-4);
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
