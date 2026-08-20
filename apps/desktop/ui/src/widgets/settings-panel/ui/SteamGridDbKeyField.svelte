<script lang="ts">
  import EyeIcon from '@lucide/svelte/icons/eye';
  import EyeOffIcon from '@lucide/svelte/icons/eye-off';
  import ExternalLinkIcon from '@lucide/svelte/icons/external-link';
  import { openExternal } from '@shared/api';
  import { Button, Input, Label, Spinner } from '@shared/ui';
  import type { SettingsMessageKind } from '@entities/settings';
  import { t } from '@shared/i18n';
  import SettingsStatusMessage from './SettingsStatusMessage.svelte';

  const STEAM_GRID_DB_KEY_PAGE_URL = 'https://www.steamgriddb.com/profile/preferences/api';
  const INPUT_ID = 'steamgriddb-api-key';
  const MESSAGE_ID = 'steamgriddb-api-key-message';

  type Props = {
    input?: string;
    loaded?: boolean;
    busy?: boolean;
    message?: string;
    messageKind?: SettingsMessageKind | null;
    onSave?: () => void;
  };

  let {
    input = $bindable(''),
    loaded = false,
    busy = false,
    message = '',
    messageKind = null,
    onSave = () => undefined,
  }: Props = $props();

  let showKey = $state(false);

  const isEditable = $derived(loaded && !busy);
  const placeholder = $derived(
    loaded ? t('settings.catalog.steamKey.placeholder') : t('settings.catalog.steamKey.loading'),
  );
  const messageId = $derived(message ? MESSAGE_ID : undefined);

  function handleSave(): void {
    if (!isEditable) {
      return;
    }
    onSave();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Enter') {
      return;
    }
    event.preventDefault();
    handleSave();
  }

  async function openKeyPage(): Promise<void> {
    try {
      await openExternal(STEAM_GRID_DB_KEY_PAGE_URL);
    } catch {
      // `openExternal` already emitted one safe diagnostic.
    }
  }
</script>

<div class="grid gap-2" aria-busy={busy}>
  <Label class="sr-only" for={INPUT_ID}>
    {t('settings.catalog.steamKey.inputLabel')}
  </Label>
  <div class="flex items-center gap-2">
    <div class="relative flex-1">
      <Input
        id={INPUT_ID}
        type={showKey ? 'text' : 'password'}
        autocomplete="off"
        class="pe-9"
        {placeholder}
        bind:value={input}
        disabled={!isEditable}
        aria-describedby={messageId}
        onkeydown={handleKeydown}
      />
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        class="absolute inset-e-1 top-1/2 -translate-y-1/2 text-muted-foreground"
        disabled={!isEditable}
        aria-label={showKey
          ? t('settings.catalog.steamKey.hide')
          : t('settings.catalog.steamKey.show')}
        onclick={() => {
          showKey = !showKey;
        }}
      >
        {#if showKey}
          <EyeOffIcon aria-hidden="true" />
        {:else}
          <EyeIcon aria-hidden="true" />
        {/if}
      </Button>
    </div>
    <Button variant="default" size="sm" disabled={!isEditable} onclick={handleSave}>
      {#if busy}
        <Spinner />
      {/if}
      {t('settings.catalog.steamKey.save')}
    </Button>
  </div>

  <div class="flex items-center gap-2">
    <SettingsStatusMessage {message} kind={messageKind} id={messageId} />
    <!-- Plain text link: strip the Button size padding, including the
         icon-specific `has-[>svg]:px-3` that `p-0` alone can't override. -->
    <Button
      type="button"
      variant="link"
      class="ms-auto h-auto gap-1 p-0 text-xs font-normal text-muted-foreground has-[>svg]:px-0"
      onclick={() => void openKeyPage()}
    >
      {t('settings.catalog.steamKey.getKey')}
      <ExternalLinkIcon class="size-3" aria-hidden="true" />
    </Button>
  </div>
</div>
