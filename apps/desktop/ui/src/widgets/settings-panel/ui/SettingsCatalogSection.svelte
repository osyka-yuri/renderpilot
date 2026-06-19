<script lang="ts">
  import { slide } from 'svelte/transition';
  import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemGroup,
    ItemSeparator,
    ItemTitle,
    Switch,
  } from '@shared/ui';
  import type { CoverRemotePolicy, SettingsMessageKind } from '@entities/settings';
  import type { CoverSourceToggleRow } from '@features/settings-artwork';
  import { t } from '@shared/i18n';
  import SettingsStatusMessage from './SettingsStatusMessage.svelte';
  import SteamGridDbKeyField from './SteamGridDbKeyField.svelte';

  type Props = {
    coverSourceToggleRows?: readonly CoverSourceToggleRow[];
    coverSourcesState?: CoverRemotePolicy;
    isCoverSourceDisabled?: (row: CoverSourceToggleRow) => boolean;
    onCoverSourceToggle?: (row: CoverSourceToggleRow) => void;
    coverSourcesMessage?: string;
    coverSourcesMessageKind?: SettingsMessageKind | null;
    steamGridDbKeyInput?: string;
    steamKeyLoaded?: boolean;
    steamKeyBusy?: boolean;
    steamKeyMessage?: string;
    steamKeyMessageKind?: SettingsMessageKind | null;
    onSteamGridDbKeySave?: () => void;
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
    coverSourcesMessageKind = null,
    steamGridDbKeyInput = $bindable(''),
    steamKeyLoaded = false,
    steamKeyBusy = false,
    steamKeyMessage = '',
    steamKeyMessageKind = null,
    onSteamGridDbKeySave = () => undefined,
  }: Props = $props();

  const isCoverSourceChecked = (row: CoverSourceToggleRow): boolean => {
    return coverSourcesState[row.policyKey];
  };

  const handleCoverSourceToggle = (row: CoverSourceToggleRow): void => {
    if (isCoverSourceDisabled(row)) {
      return;
    }
    onCoverSourceToggle(row);
  };
</script>

<Card>
  <CardHeader>
    <CardTitle>{t('settings.catalog.title')}</CardTitle>
    <CardDescription>{t('settings.catalog.description')}</CardDescription>
  </CardHeader>
  <CardContent>
    <ItemGroup>
      {#each coverSourceToggleRows as row, index (row.settingKey)}
        {#if index > 0}
          <ItemSeparator />
        {/if}
        <Item>
          <ItemContent>
            <ItemTitle>{t(row.titleKey)}</ItemTitle>
            <ItemDescription>{t(row.descriptionKey)}</ItemDescription>
          </ItemContent>
          <ItemActions>
            <Switch
              checked={isCoverSourceChecked(row)}
              disabled={isCoverSourceDisabled(row)}
              aria-label={t(row.ariaLabelKey)}
              onCheckedChange={() => {
                handleCoverSourceToggle(row);
              }}
            />
          </ItemActions>
        </Item>

        {#if row.policyKey === 'steamgriddb' && coverSourcesState.steamgriddb}
          <div transition:slide={{ duration: 150 }}>
            <Item class="pt-0">
              <ItemContent class="gap-2" aria-busy={steamKeyBusy}>
                <SteamGridDbKeyField
                  bind:input={steamGridDbKeyInput}
                  loaded={steamKeyLoaded}
                  busy={steamKeyBusy}
                  message={steamKeyMessage}
                  messageKind={steamKeyMessageKind}
                  onSave={onSteamGridDbKeySave}
                />
              </ItemContent>
            </Item>
          </div>
        {/if}
      {/each}

      <SettingsStatusMessage message={coverSourcesMessage} kind={coverSourcesMessageKind} />
    </ItemGroup>
  </CardContent>
</Card>
