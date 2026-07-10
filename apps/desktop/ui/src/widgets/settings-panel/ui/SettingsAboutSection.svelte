<script lang="ts">
  import CircleArrowUpIcon from '@lucide/svelte/icons/circle-arrow-up';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';

  import {
    Button,
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
    ItemTitle,
    Spinner,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import type { SettingsUpdateAction } from '@features/app-updater';

  type Props = {
    appVersion?: string | null;
    updateAction?: SettingsUpdateAction;
    onCheckForUpdates?: () => void;
  };

  const {
    appVersion = null,
    updateAction = 'check',
    onCheckForUpdates = () => undefined,
  }: Props = $props();

  const isDisabled = $derived(updateAction === 'checking' || updateAction === 'busy');
  const showSpinner = $derived(updateAction === 'checking' || updateAction === 'busy');

  const buttonLabel = $derived.by(() => {
    switch (updateAction) {
      case 'checking':
        return t('settings.about.checkingForUpdates');
      case 'open-update':
        return t('settings.about.updateAvailable');
      case 'busy':
        return t('settings.about.updateInProgress');
      case 'check':
      default:
        return t('settings.about.checkForUpdates');
    }
  });
</script>

<Card>
  <CardHeader>
    <CardTitle>{t('settings.about.title')}</CardTitle>
    <CardDescription>{t('settings.about.description')}</CardDescription>
  </CardHeader>
  <CardContent>
    <ItemGroup>
      <Item>
        <ItemContent>
          <ItemTitle>{t('settings.about.version.title')}</ItemTitle>
          <ItemDescription>
            {#if appVersion}
              RenderPilot v{appVersion}
            {:else}
              {t('settings.about.version.loading')}
            {/if}
          </ItemDescription>
        </ItemContent>
        <ItemActions>
          <Button variant="secondary" size="sm" disabled={isDisabled} onclick={onCheckForUpdates}>
            {#if showSpinner}
              <Spinner class="mr-2" />
            {:else if updateAction === 'open-update'}
              <CircleArrowUpIcon class="mr-2" aria-hidden="true" />
            {:else}
              <RefreshCwIcon class="mr-2" aria-hidden="true" />
            {/if}
            {buttonLabel}
          </Button>
        </ItemActions>
      </Item>
    </ItemGroup>
  </CardContent>
</Card>
