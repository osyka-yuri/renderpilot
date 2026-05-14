<script lang="ts">
  import {
    Button,
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
    Input,
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemGroup,
    ItemSeparator,
    ItemTitle,
    Switch,
  } from '@shared/ui';
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
  }: Props = $props();

  const isSteamKeyEditable = $derived(steamKeyLoaded && !steamKeyBusy);
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

  function handleSteamGridDbKeySave(): void {
    if (!isSteamKeyEditable) {
      return;
    }
    onSteamGridDbKeySave();
  }
</script>

<Card>
  <CardHeader>
    <CardTitle>Cover sources</CardTitle>
    <CardDescription
      >Choose which remote sources may run when downloading artwork automatically.</CardDescription
    >
  </CardHeader>
  <CardContent>
    <ItemGroup>
      {#each coverSourceToggleRows as row, index (row.settingKey)}
        {#if index > 0}
          <ItemSeparator />
        {/if}
        <Item>
          <ItemContent>
            <ItemTitle>{row.title}</ItemTitle>
            <ItemDescription>{row.description}</ItemDescription>
          </ItemContent>
          <ItemActions>
            <Switch
              checked={isCoverSourceChecked(row)}
              disabled={isCoverSourceDisabled(row)}
              aria-label={row.ariaLabel}
              onCheckedChange={() => {
                handleCoverSourceToggle(row);
              }}
            />
          </ItemActions>
        </Item>

        {#if row.policyKey === 'steamgriddb'}
          <div class="grid w-full max-w-88 gap-2 px-4" aria-busy={steamKeyBusy}>
            <label class="sr-only" for={STEAM_GRID_DB_KEY_INPUT_ID}>SteamGridDB API key</label>
            <div class="flex items-center gap-2">
              <Input
                id={STEAM_GRID_DB_KEY_INPUT_ID}
                type="password"
                autocomplete="off"
                placeholder={steamKeyPlaceholder}
                bind:value={steamGridDbKeyInput}
                disabled={!isSteamKeyEditable}
                aria-describedby={steamKeyMessageId}
              />
              <Button
                variant="default"
                size="sm"
                disabled={!isSteamKeyEditable}
                onclick={handleSteamGridDbKeySave}
              >
                Save
              </Button>
            </div>

            {#if steamKeyMessage}
              <p
                id={STEAM_GRID_DB_KEY_MESSAGE_ID}
                class="text-xs text-muted-foreground"
                role="status"
                aria-live="polite"
              >
                {steamKeyMessage}
              </p>
            {/if}
          </div>
        {/if}
      {/each}

      {#if coverSourcesMessage}
        <p class="text-xs text-muted-foreground" role="status" aria-live="polite">
          {coverSourcesMessage}
        </p>
      {/if}
    </ItemGroup>
  </CardContent>
</Card>
